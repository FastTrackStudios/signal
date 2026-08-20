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
use architect::{layers, HasDispatcher, Layer, PubSub, Services};
use daw_audio_io::AudioIoPrefs;
use midicore::MidiEvent;
use signal_keys_proto::keys::{KeysEvent, KeysRig as KeysRigSvc, KeysRigStreamSource};
use signal_keys_proto::{
    KeysEngineDetail, KeysEngineModel, KeysLayerDetail, KeysLayerModel, KeysMacro, KeysMeter,
    KeysMixer, KeysNode, KeysPerform, KeysPreset, KeysStack, KeysStatus,
};

use crate::profile::{worship_profile, KeysProfile};
use signal_rig_host::mixer::{self as rig_mixer, db_to_linear};
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
    /// The RIG's own Global Controls — the level above the engines, over
    /// every module in the profile. Every engine exposes the same macro
    /// surface, so one panel drives all of them.
    rig_globals: BTreeMap<String, f32>,
    /// The rig level's live bipolar offsets — see [`LaneState::spans`].
    rig_spans: BTreeMap<String, (f32, Baseline)>,
    /// Index of the last pressed stack.
    active_stack: Option<usize>,
    /// Grid mode: 0 Preset, 1 Profile (stacks), 2 Setlist.
    perform_mode: u32,
}

/// What the program builders read: the live profile, the patch-name →
/// spec-path index, and the lane snapshot.
type ProfileInputs = (
    KeysProfile,
    BTreeMap<String, PathBuf>,
    BTreeMap<String, LaneState>,
);

/// Live per-layer mixer state (the profile holds the authored defaults).
#[derive(Clone, Debug)]
struct LaneState {
    engine: String,
    gain_db: f32,
    muted: bool,
    soloed: bool,
    /// The lane's four modules (the engine instances). Module A is index 0.
    modules: Vec<ModuleState>,
    /// The module preset this lane was opened from ("American Obesity"), or
    /// empty when its modules were assembled by hand.
    preset: String,
    /// Layer macro values that belong to the layer itself (Tone, Limiter, FX
    /// bypass) — the ones with no module target.
    globals: BTreeMap<String, f32>,
    /// Live bipolar offsets: layer macro id → (offset −1..1, the module
    /// values it started from). The baseline is what the detent returns to,
    /// so a Global Control never destroys the patch's own settings.
    spans: BTreeMap<String, (f32, Baseline)>,
}

/// What a Global Control captured when it left centre: the value each module
/// it drives held at that moment, addressed by `(lane, module index)` so the
/// same shape serves a layer's knob and an engine's.
type Baseline = Vec<(String, usize, f32)>;

/// One module: its Source Block's patch and its own macro values (each
/// module has its own filter, amp envelope and FX).
#[derive(Clone, Debug)]
struct ModuleState {
    /// The soundsource in the Source Block — what actually loads, and what
    /// the module is called.
    patch: String,
    /// The chosen variation of that source ("Rock"), empty for the default.
    /// Variations share the soundsource, so this changes no audio yet — it is
    /// the state the authored parameter sets will apply over.
    variant: String,
    macros: BTreeMap<String, f32>,
    gain_db: f32,
    enabled: bool,
}

impl Default for ModuleState {
    fn default() -> Self {
        Self {
            patch: String::new(),
            variant: String::new(),
            macros: BTreeMap::new(),
            gain_db: 0.0,
            enabled: true,
        }
    }
}

impl LaneState {
    /// What the mixer shows for the lane: the module preset it was opened
    /// from, else module A's soundsource.
    fn primary_patch(&self) -> String {
        if !self.preset.is_empty() {
            return self.preset.clone();
        }
        self.modules
            .first()
            .map(|m| m.patch.clone())
            .unwrap_or_default()
    }

    /// Any module sounding?
    fn any_live(&self) -> bool {
        self.modules
            .iter()
            .any(|m| !m.patch.is_empty() && m.enabled)
    }

    /// The linear gain a module renders at (off / empty = silent).
    fn module_gain(&self, index: usize) -> f32 {
        match self.modules.get(index) {
            Some(m) if m.enabled && !m.patch.is_empty() => {
                // Pack normalization: the library's own mastering level is
                // corrected here so the fader above it is a mix decision
                // rather than a calibration one.
                db_to_linear(m.gain_db + crate::normalize::trim_db(&m.patch))
            }
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
    MacroDef {
        id: "source.level",
        name: "Level",
        group: "Source",
        default: 0.0,
        min: -24.0,
        max: 12.0,
        unit: "dB",
        live: true,
    },
    MacroDef {
        id: "source.pan",
        name: "Pan",
        group: "Source",
        default: 0.0,
        min: -1.0,
        max: 1.0,
        unit: "",
        live: false,
    },
    MacroDef {
        id: "source.transpose",
        name: "Transpose",
        group: "Source",
        default: 0.0,
        min: -24.0,
        max: 24.0,
        unit: "st",
        live: false,
    },
    MacroDef {
        id: "source.fine",
        name: "Fine",
        group: "Source",
        default: 0.0,
        min: -100.0,
        max: 100.0,
        unit: "c",
        live: false,
    },
    MacroDef {
        id: "source.unison",
        name: "Unison",
        group: "Source",
        default: 1.0,
        min: 1.0,
        max: 8.0,
        unit: "v",
        live: true,
    },
    MacroDef {
        id: "source.detune",
        name: "Detune",
        group: "Source",
        default: 0.1,
        min: 0.0,
        max: 2.0,
        unit: "",
        live: true,
    },
    // ── Filter ──────────────────────────────────────────────────────────
    MacroDef {
        id: "filter.cutoff",
        name: "Cutoff",
        group: "Filter",
        default: 20000.0,
        min: 20.0,
        max: 20000.0,
        unit: "Hz",
        live: true,
    },
    MacroDef {
        id: "filter.reso",
        name: "Resonance",
        group: "Filter",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        unit: "",
        live: true,
    },
    MacroDef {
        id: "filter.env_amt",
        name: "Env Amt",
        group: "Filter",
        default: 0.0,
        min: -1.0,
        max: 1.0,
        unit: "",
        live: true,
    },
    MacroDef {
        id: "filter.keytrack",
        name: "Key Trk",
        group: "Filter",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        unit: "",
        live: false,
    },
    MacroDef {
        id: "filter.drive",
        name: "Drive",
        group: "Filter",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        unit: "",
        live: false,
    },
    MacroDef {
        id: "filter.mix",
        name: "Mix",
        group: "Filter",
        default: 1.0,
        min: 0.0,
        max: 1.0,
        unit: "",
        live: false,
    },
    // ── Envelopes 1..4 ──────────────────────────────────────────────────
    // ENV 1 is bound to the Amp and ENV 2 to the Filter (the bindings the
    // engine assumes today; unbinding lands with the mod matrix). 3 and 4
    // are free — route them from the matrix.
    MacroDef {
        id: "env1.delay",
        name: "Delay",
        group: "Env 1",
        default: 0.0,
        min: 0.0,
        max: 2000.0,
        unit: "ms",
        live: false,
    },
    MacroDef {
        id: "env1.attack",
        name: "Attack",
        group: "Env 1",
        default: 0.0,
        min: 0.0,
        max: 5000.0,
        unit: "ms",
        live: true,
    },
    MacroDef {
        id: "env1.hold",
        name: "Hold",
        group: "Env 1",
        default: 0.0,
        min: 0.0,
        max: 2000.0,
        unit: "ms",
        live: false,
    },
    MacroDef {
        id: "env1.decay",
        name: "Decay",
        group: "Env 1",
        default: 0.0,
        min: 0.0,
        max: 5000.0,
        unit: "ms",
        live: true,
    },
    MacroDef {
        id: "env1.sustain",
        name: "Sustain",
        group: "Env 1",
        default: 1.0,
        min: 0.0,
        max: 1.0,
        unit: "",
        live: true,
    },
    MacroDef {
        id: "env1.release",
        name: "Release",
        group: "Env 1",
        default: 120.0,
        min: 0.0,
        max: 8000.0,
        unit: "ms",
        live: true,
    },
    MacroDef {
        id: "env2.delay",
        name: "Delay",
        group: "Env 2",
        default: 0.0,
        min: 0.0,
        max: 2000.0,
        unit: "ms",
        live: false,
    },
    MacroDef {
        id: "env2.attack",
        name: "Attack",
        group: "Env 2",
        default: 5.0,
        min: 0.0,
        max: 5000.0,
        unit: "ms",
        live: true,
    },
    MacroDef {
        id: "env2.hold",
        name: "Hold",
        group: "Env 2",
        default: 0.0,
        min: 0.0,
        max: 2000.0,
        unit: "ms",
        live: false,
    },
    MacroDef {
        id: "env2.decay",
        name: "Decay",
        group: "Env 2",
        default: 300.0,
        min: 0.0,
        max: 5000.0,
        unit: "ms",
        live: true,
    },
    MacroDef {
        id: "env2.sustain",
        name: "Sustain",
        group: "Env 2",
        default: 0.7,
        min: 0.0,
        max: 1.0,
        unit: "",
        live: true,
    },
    MacroDef {
        id: "env2.release",
        name: "Release",
        group: "Env 2",
        default: 200.0,
        min: 0.0,
        max: 8000.0,
        unit: "ms",
        live: true,
    },
    MacroDef {
        id: "env3.delay",
        name: "Delay",
        group: "Env 3",
        default: 0.0,
        min: 0.0,
        max: 2000.0,
        unit: "ms",
        live: false,
    },
    MacroDef {
        id: "env3.attack",
        name: "Attack",
        group: "Env 3",
        default: 20.0,
        min: 0.0,
        max: 5000.0,
        unit: "ms",
        live: false,
    },
    MacroDef {
        id: "env3.hold",
        name: "Hold",
        group: "Env 3",
        default: 0.0,
        min: 0.0,
        max: 2000.0,
        unit: "ms",
        live: false,
    },
    MacroDef {
        id: "env3.decay",
        name: "Decay",
        group: "Env 3",
        default: 400.0,
        min: 0.0,
        max: 5000.0,
        unit: "ms",
        live: false,
    },
    MacroDef {
        id: "env3.sustain",
        name: "Sustain",
        group: "Env 3",
        default: 0.5,
        min: 0.0,
        max: 1.0,
        unit: "",
        live: false,
    },
    MacroDef {
        id: "env3.release",
        name: "Release",
        group: "Env 3",
        default: 300.0,
        min: 0.0,
        max: 8000.0,
        unit: "ms",
        live: false,
    },
    MacroDef {
        id: "env4.delay",
        name: "Delay",
        group: "Env 4",
        default: 0.0,
        min: 0.0,
        max: 2000.0,
        unit: "ms",
        live: false,
    },
    MacroDef {
        id: "env4.attack",
        name: "Attack",
        group: "Env 4",
        default: 200.0,
        min: 0.0,
        max: 5000.0,
        unit: "ms",
        live: false,
    },
    MacroDef {
        id: "env4.hold",
        name: "Hold",
        group: "Env 4",
        default: 0.0,
        min: 0.0,
        max: 2000.0,
        unit: "ms",
        live: false,
    },
    MacroDef {
        id: "env4.decay",
        name: "Decay",
        group: "Env 4",
        default: 600.0,
        min: 0.0,
        max: 5000.0,
        unit: "ms",
        live: false,
    },
    MacroDef {
        id: "env4.sustain",
        name: "Sustain",
        group: "Env 4",
        default: 0.6,
        min: 0.0,
        max: 1.0,
        unit: "",
        live: false,
    },
    MacroDef {
        id: "env4.release",
        name: "Release",
        group: "Env 4",
        default: 500.0,
        min: 0.0,
        max: 8000.0,
        unit: "ms",
        live: false,
    },
    // ── LFOs 1..4 ───────────────────────────────────────────────────────
    MacroDef {
        id: "lfo1.rate",
        name: "Rate",
        group: "LFO 1",
        default: 2.0,
        min: 0.01,
        max: 40.0,
        unit: "Hz",
        live: false,
    },
    MacroDef {
        id: "lfo1.depth",
        name: "Depth",
        group: "LFO 1",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        unit: "",
        live: false,
    },
    MacroDef {
        id: "lfo1.shape",
        name: "Shape",
        group: "LFO 1",
        default: 0.0,
        min: 0.0,
        max: 4.0,
        unit: "",
        live: false,
    },
    MacroDef {
        id: "lfo1.fade",
        name: "Fade In",
        group: "LFO 1",
        default: 0.0,
        min: 0.0,
        max: 4000.0,
        unit: "ms",
        live: false,
    },
    MacroDef {
        id: "lfo2.rate",
        name: "Rate",
        group: "LFO 2",
        default: 0.5,
        min: 0.01,
        max: 40.0,
        unit: "Hz",
        live: false,
    },
    MacroDef {
        id: "lfo2.depth",
        name: "Depth",
        group: "LFO 2",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        unit: "",
        live: false,
    },
    MacroDef {
        id: "lfo2.shape",
        name: "Shape",
        group: "LFO 2",
        default: 1.0,
        min: 0.0,
        max: 4.0,
        unit: "",
        live: false,
    },
    MacroDef {
        id: "lfo2.fade",
        name: "Fade In",
        group: "LFO 2",
        default: 0.0,
        min: 0.0,
        max: 4000.0,
        unit: "ms",
        live: false,
    },
    MacroDef {
        id: "lfo3.rate",
        name: "Rate",
        group: "LFO 3",
        default: 4.0,
        min: 0.01,
        max: 40.0,
        unit: "Hz",
        live: false,
    },
    MacroDef {
        id: "lfo3.depth",
        name: "Depth",
        group: "LFO 3",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        unit: "",
        live: false,
    },
    MacroDef {
        id: "lfo3.shape",
        name: "Shape",
        group: "LFO 3",
        default: 2.0,
        min: 0.0,
        max: 4.0,
        unit: "",
        live: false,
    },
    MacroDef {
        id: "lfo3.fade",
        name: "Fade In",
        group: "LFO 3",
        default: 0.0,
        min: 0.0,
        max: 4000.0,
        unit: "ms",
        live: false,
    },
    MacroDef {
        id: "lfo4.rate",
        name: "Rate",
        group: "LFO 4",
        default: 8.0,
        min: 0.01,
        max: 40.0,
        unit: "Hz",
        live: false,
    },
    MacroDef {
        id: "lfo4.depth",
        name: "Depth",
        group: "LFO 4",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        unit: "",
        live: false,
    },
    MacroDef {
        id: "lfo4.shape",
        name: "Shape",
        group: "LFO 4",
        default: 3.0,
        min: 0.0,
        max: 4.0,
        unit: "",
        live: false,
    },
    MacroDef {
        id: "lfo4.fade",
        name: "Fade In",
        group: "LFO 4",
        default: 0.0,
        min: 0.0,
        max: 4000.0,
        unit: "ms",
        live: false,
    },
    // ── Tone / Vibrato / Ambience / Effects (per module) ─────────────────
    MacroDef {
        id: "tone.warmth",
        name: "Warmth",
        group: "Tone",
        default: 0.5,
        min: 0.0,
        max: 1.0,
        unit: "",
        live: false,
    },
    MacroDef {
        id: "tone.drive",
        name: "Drive",
        group: "Tone",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        unit: "",
        live: false,
    },
    MacroDef {
        id: "tone.body",
        name: "Body",
        group: "Tone",
        default: 0.5,
        min: 0.0,
        max: 1.0,
        unit: "",
        live: false,
    },
    MacroDef {
        id: "vib.rate",
        name: "Rate",
        group: "Vibrato",
        default: 5.0,
        min: 0.1,
        max: 12.0,
        unit: "Hz",
        live: true,
    },
    MacroDef {
        id: "vib.depth",
        name: "Depth",
        group: "Vibrato",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        unit: "",
        live: true,
    },
    MacroDef {
        id: "vib.delay",
        name: "Delay",
        group: "Vibrato",
        default: 300.0,
        min: 0.0,
        max: 3000.0,
        unit: "ms",
        live: false,
    },
    MacroDef {
        id: "amb.bypass",
        name: "Bypass",
        group: "Ambience",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        unit: "",
        live: false,
    },
    MacroDef {
        id: "amb.algo",
        name: "Algorithm",
        group: "Ambience",
        default: 1.0,
        min: 0.0,
        max: 14.0,
        unit: "",
        live: false,
    },
    MacroDef {
        id: "amb.size",
        name: "Size",
        group: "Ambience",
        default: 0.5,
        min: 0.0,
        max: 1.0,
        unit: "",
        live: false,
    },
    MacroDef {
        id: "amb.mix",
        name: "Mix",
        group: "Ambience",
        default: 0.15,
        min: 0.0,
        max: 1.0,
        unit: "",
        live: false,
    },
    MacroDef {
        id: "amb.predelay",
        name: "Pre-dly",
        group: "Ambience",
        default: 20.0,
        min: 0.0,
        max: 250.0,
        unit: "ms",
        live: false,
    },
    MacroDef {
        id: "amb.decay",
        name: "Decay",
        group: "Ambience",
        default: 0.45,
        min: 0.02,
        max: 1.0,
        unit: "",
        live: false,
    },
    // The delay is its own section, not an Effects amount: it is the other
    // half of the time-domain picture the band draws, and it needs a time and
    // a feedback to draw at all.
    // 0 = free (use `dly.time`); 1..7 are note divisions against the tempo —
    // a delay is set musically far more often than in milliseconds.
    // Machine / algorithm: an index into the rig's tables (see
    // signal-keys-ui's `algos`), the same shape the guitar rig's picker writes.
    // 0 engaged, 1 bypassed — the same sense as the engine lamp beside it.
    MacroDef {
        id: "dly.bypass",
        name: "Bypass",
        group: "Delay",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        unit: "",
        live: false,
    },
    MacroDef {
        id: "dly.algo",
        name: "Machine",
        group: "Delay",
        default: 0.0,
        min: 0.0,
        max: 12.0,
        unit: "",
        live: false,
    },
    MacroDef {
        id: "dly.div",
        name: "Div",
        group: "Delay",
        default: 3.0,
        min: 0.0,
        max: 7.0,
        unit: "",
        live: false,
    },
    MacroDef {
        id: "dly.time",
        name: "Time",
        group: "Delay",
        default: 375.0,
        min: 20.0,
        max: 2000.0,
        unit: "ms",
        live: false,
    },
    MacroDef {
        id: "dly.feedback",
        name: "Feedback",
        group: "Delay",
        default: 0.35,
        min: 0.0,
        max: 0.95,
        unit: "",
        live: false,
    },
    MacroDef {
        id: "dly.mix",
        name: "Mix",
        group: "Delay",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        unit: "",
        live: false,
    },
    MacroDef {
        id: "fx.chorus",
        name: "Chorus",
        group: "Effects",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        unit: "",
        live: false,
    },
    MacroDef {
        id: "fx.delay",
        name: "Delay",
        group: "Effects",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        unit: "",
        live: false,
    },
    MacroDef {
        id: "fx.width",
        name: "Width",
        group: "Effects",
        default: 0.5,
        min: 0.0,
        max: 1.0,
        unit: "",
        live: false,
    },
];

/// One **Global Control** — Omnisphere's Main-page model: a knob that drives
/// the same parameter on every audible module beneath it.
///
/// The same table serves both levels of the tree. A layer's copy of a control
/// is `l.<key>` and reaches its own modules; an engine's is `e.<key>` and
/// reaches every module in every one of its lanes. Each level keeps its own
/// value and its own offset span, so the engine knob is an offset over the
/// layer knobs' results exactly as a layer knob is one over its modules'.
///
/// `target` names the module macro it reaches. A `None` target is the scope's
/// own value (its EQ / limiter sit at the output, not inside a module), which
/// is exactly how Omnisphere treats TONE and the LIMITER: part-level, after
/// everything beneath has summed.
struct GlobalDef {
    /// Id without its level prefix ("filter.cutoff" → `l.filter.cutoff` /
    /// `e.filter.cutoff`).
    key: &'static str,
    name: &'static str,
    group: &'static str,
    /// The module macro this drives, or `None` for a scope-owned parameter.
    target: Option<&'static str>,
    default: f32,
    min: f32,
    max: f32,
    unit: &'static str,
    live: bool,
}

/// The Global Controls, in panel order — Filter, Envelope (Amp + Filter
/// ADSR), Vibrato, Unison, Ambience, then the scope's own Tone / Effects /
/// Limiter.
const GLOBALS: &[GlobalDef] = &[
    // ── Filter ───────────────────────────────────────────────────────────
    GlobalDef {
        key: "filter.cutoff",
        name: "Cutoff",
        group: "Filter",
        target: Some("filter.cutoff"),
        default: 20000.0,
        min: 20.0,
        max: 20000.0,
        unit: "Hz",
        live: false,
    },
    GlobalDef {
        key: "filter.reso",
        name: "Resonance",
        group: "Filter",
        target: Some("filter.reso"),
        default: 0.0,
        min: 0.0,
        max: 1.0,
        unit: "",
        live: false,
    },
    GlobalDef {
        key: "filter.env",
        name: "Env Amt",
        group: "Filter",
        target: Some("filter.env_amt"),
        default: 0.0,
        min: -1.0,
        max: 1.0,
        unit: "",
        live: false,
    },
    // ── Envelope: the Amp ADSR (ENV 1) then the Filter ADSR (ENV 2) ──────
    GlobalDef {
        key: "amp.attack",
        name: "A",
        group: "Amp Env",
        target: Some("env1.attack"),
        default: 0.0,
        min: 0.0,
        max: 5000.0,
        unit: "ms",
        live: true,
    },
    GlobalDef {
        key: "amp.decay",
        name: "D",
        group: "Amp Env",
        target: Some("env1.decay"),
        default: 0.0,
        min: 0.0,
        max: 5000.0,
        unit: "ms",
        live: false,
    },
    GlobalDef {
        key: "amp.sustain",
        name: "S",
        group: "Amp Env",
        target: Some("env1.sustain"),
        default: 1.0,
        min: 0.0,
        max: 1.0,
        unit: "",
        live: false,
    },
    GlobalDef {
        key: "amp.release",
        name: "R",
        group: "Amp Env",
        target: Some("env1.release"),
        default: 120.0,
        min: 0.0,
        max: 8000.0,
        unit: "ms",
        live: true,
    },
    GlobalDef {
        key: "fenv.attack",
        name: "A",
        group: "Filter Env",
        target: Some("env2.attack"),
        default: 0.0,
        min: 0.0,
        max: 5000.0,
        unit: "ms",
        live: false,
    },
    GlobalDef {
        key: "fenv.decay",
        name: "D",
        group: "Filter Env",
        target: Some("env2.decay"),
        default: 0.0,
        min: 0.0,
        max: 5000.0,
        unit: "ms",
        live: false,
    },
    GlobalDef {
        key: "fenv.sustain",
        name: "S",
        group: "Filter Env",
        target: Some("env2.sustain"),
        default: 1.0,
        min: 0.0,
        max: 1.0,
        unit: "",
        live: false,
    },
    GlobalDef {
        key: "fenv.release",
        name: "R",
        group: "Filter Env",
        target: Some("env2.release"),
        default: 120.0,
        min: 0.0,
        max: 8000.0,
        unit: "ms",
        live: false,
    },
    // ── Vibrato ──────────────────────────────────────────────────────────
    GlobalDef {
        key: "vib.rate",
        name: "Rate",
        group: "Vibrato",
        target: Some("vib.rate"),
        default: 5.0,
        min: 0.1,
        max: 12.0,
        unit: "Hz",
        live: false,
    },
    GlobalDef {
        key: "vib.depth",
        name: "Depth",
        group: "Vibrato",
        target: Some("vib.depth"),
        default: 0.0,
        min: 0.0,
        max: 1.0,
        unit: "",
        live: false,
    },
    // ── Unison ───────────────────────────────────────────────────────────
    GlobalDef {
        key: "uni.voices",
        name: "Voices",
        group: "Unison",
        target: Some("source.unison"),
        default: 1.0,
        min: 1.0,
        max: 8.0,
        unit: "v",
        live: true,
    },
    GlobalDef {
        key: "uni.detune",
        name: "Detune",
        group: "Unison",
        target: Some("source.detune"),
        default: 0.1,
        min: 0.0,
        max: 2.0,
        unit: "",
        live: true,
    },
    // ── Ambience ─────────────────────────────────────────────────────────
    GlobalDef {
        key: "amb.bypass",
        name: "Bypass",
        group: "Ambience",
        target: Some("amb.bypass"),
        default: 0.0,
        min: 0.0,
        max: 1.0,
        unit: "",
        live: false,
    },
    GlobalDef {
        key: "amb.algo",
        name: "Algorithm",
        group: "Ambience",
        target: Some("amb.algo"),
        default: 1.0,
        min: 0.0,
        max: 14.0,
        unit: "",
        live: false,
    },
    GlobalDef {
        key: "amb.amount",
        name: "Amount",
        group: "Ambience",
        target: Some("amb.mix"),
        default: 0.15,
        min: 0.0,
        max: 1.0,
        unit: "",
        live: false,
    },
    GlobalDef {
        key: "amb.length",
        name: "Length",
        group: "Ambience",
        target: Some("amb.size"),
        default: 0.5,
        min: 0.0,
        max: 1.0,
        unit: "",
        live: false,
    },
    GlobalDef {
        key: "amb.decay",
        name: "Decay",
        group: "Ambience",
        target: Some("amb.decay"),
        default: 0.45,
        min: 0.02,
        max: 1.0,
        unit: "",
        live: false,
    },
    // ── Delay, at every level: the rig's tail, an engine's, a lane's ─────
    GlobalDef {
        key: "dly.bypass",
        name: "Bypass",
        group: "Delay",
        target: Some("dly.bypass"),
        default: 0.0,
        min: 0.0,
        max: 1.0,
        unit: "",
        live: false,
    },
    GlobalDef {
        key: "dly.algo",
        name: "Machine",
        group: "Delay",
        target: Some("dly.algo"),
        default: 0.0,
        min: 0.0,
        max: 12.0,
        unit: "",
        live: false,
    },
    GlobalDef {
        key: "dly.div",
        name: "Div",
        group: "Delay",
        target: Some("dly.div"),
        default: 3.0,
        min: 0.0,
        max: 7.0,
        unit: "",
        live: false,
    },
    GlobalDef {
        key: "dly.time",
        name: "Time",
        group: "Delay",
        target: Some("dly.time"),
        default: 375.0,
        min: 20.0,
        max: 2000.0,
        unit: "ms",
        live: false,
    },
    GlobalDef {
        key: "dly.feedback",
        name: "Feedback",
        group: "Delay",
        target: Some("dly.feedback"),
        default: 0.35,
        min: 0.0,
        max: 0.95,
        unit: "",
        live: false,
    },
    GlobalDef {
        key: "dly.mix",
        name: "Mix",
        group: "Delay",
        target: Some("dly.mix"),
        default: 0.0,
        min: 0.0,
        max: 1.0,
        unit: "",
        live: false,
    },
    // ── Tone: the scope's EQ, centred = bypassed ─────────────────────────
    GlobalDef {
        key: "tone.low",
        name: "Low",
        group: "Tone",
        target: None,
        default: 0.0,
        min: -12.0,
        max: 12.0,
        unit: "dB",
        live: false,
    },
    GlobalDef {
        key: "tone.mid",
        name: "Mid",
        group: "Tone",
        target: None,
        default: 0.0,
        min: -12.0,
        max: 12.0,
        unit: "dB",
        live: false,
    },
    GlobalDef {
        key: "tone.high",
        name: "High",
        group: "Tone",
        target: None,
        default: 0.0,
        min: -12.0,
        max: 12.0,
        unit: "dB",
        live: false,
    },
    // ── Effects + Limiter, at the scope's output ─────────────────────────
    GlobalDef {
        key: "fx.bypass",
        name: "Bypass",
        group: "Effects",
        target: None,
        default: 0.0,
        min: 0.0,
        max: 1.0,
        unit: "",
        live: false,
    },
    GlobalDef {
        key: "limiter",
        name: "Limiter",
        group: "Effects",
        target: None,
        default: 0.0,
        min: 0.0,
        max: 1.0,
        unit: "",
        live: false,
    },
];

/// Id prefixes for the three levels a Global Control lives at. The wire ids
/// are `l.filter.cutoff` (layer), `e.filter.cutoff` (engine) and
/// `r.filter.cutoff` (the whole rig).
const LAYER: &str = "l.";
const ENGINE: &str = "e.";
const RIG: &str = "r.";

/// The definition behind a Global Control id, at any level.
fn global_def(id: &str) -> Option<&'static GlobalDef> {
    let key = id
        .strip_prefix(LAYER)
        .or_else(|| id.strip_prefix(ENGINE))
        .or_else(|| id.strip_prefix(RIG))?;
    GLOBALS.iter().find(|g| g.key == key)
}

/// How a value reads back in the UI's spread text.
fn fmt_value(v: f32, unit: &str) -> String {
    match unit {
        "Hz" if v >= 1000.0 => format!("{:.1} kHz", v / 1000.0),
        "Hz" => format!("{v:.0} Hz"),
        "ms" if v >= 1000.0 => format!("{:.2} s", v / 1000.0),
        "ms" => format!("{v:.0} ms"),
        "" => format!("{v:.2}"),
        u => format!("{v:.1} {u}"),
    }
}

fn macro_def(id: &str) -> Option<&'static MacroDef> {
    MACROS.iter().find(|m| m.id == id)
}

fn default_macros() -> BTreeMap<String, f32> {
    MACROS
        .iter()
        .map(|m| (m.id.to_string(), m.default))
        .collect()
}

/// Live per-engine mixer state — its trim, its bypass, and the level of
/// Global Controls that sits above all of its lanes.
#[derive(Clone, Debug, Default)]
struct EngineState {
    gain_db: f32,
    muted: bool,
    /// A drone engine's held note: pitch class (0 = C), the octave it sounds
    /// in, and whether it is sounding. `None` for engines that are played.
    drone: Option<signal_keys_proto::KeysDrone>,
    /// Engine macro values that belong to the engine itself (its Tone,
    /// Limiter, FX bypass) — the ones with no module target.
    globals: BTreeMap<String, f32>,
    /// Live bipolar offsets over the whole engine, exactly as a lane's.
    spans: BTreeMap<String, (f32, Baseline)>,
}

impl State {
    /// Seed the live mixer from a profile's authored defaults.
    fn adopt_profile(&mut self, profile: KeysProfile) {
        self.lanes.clear();
        self.engines.clear();
        for engine in &profile.engines {
            self.engines.insert(
                engine.name.clone(),
                EngineState {
                    gain_db: engine.gain_db,
                    // A drone starts silent and empty: it holds a note under
                    // the band when a player asks for one, and a drone that
                    // switched itself on at boot would be a fault, not a
                    // feature.
                    muted: is_drone(&engine.name),
                    ..EngineState::default()
                },
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
                        preset: String::new(),
                        globals: BTreeMap::new(),
                        spans: BTreeMap::new(),
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
        let Some(lane) = self.lanes.get(name) else {
            return 0.0;
        };
        let engine_muted = self.engines.get(&lane.engine).is_some_and(|e| e.muted);
        let mix = rig_mixer::LaneMix {
            gain_db: lane.gain_db,
            muted: lane.muted,
            soloed: lane.soloed,
            live: lane.any_live(),
        };
        rig_mixer::lane_gain(&mix, engine_muted, self.any_solo())
    }

    fn engine_gain(&self, name: &str) -> f32 {
        match self.engines.get(name) {
            Some(e) => rig_mixer::group_gain(e.gain_db, e.muted),
            None => 1.0,
        }
    }
}

struct Inner {
    rig: Mutex<Option<KeysRig>>,
    state: Mutex<State>,
    events: PubSub<KeysEvent>,
    pump_started: AtomicBool,
    /// Bumped by every DSP-parameter edit; a coalescing rebuild only runs if
    /// it is still the latest when its wait is up (see `rebuild_soon`).
    rebuild_gen: std::sync::atomic::AtomicU64,
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
    /// What a module currently holds for `id` (its own value, else the
    /// macro's default).
    fn module_value(lane: &LaneState, index: usize, id: &str) -> f32 {
        lane.modules
            .get(index)
            .and_then(|m| m.macros.get(id).copied())
            .or_else(|| macro_def(id).map(|d| d.default))
            .unwrap_or(0.0)
    }

    /// A module's envelope in macro units (its own values, else defaults).
    fn module_env(lane: &LaneState, index: usize, prefix: &str) -> signal_keys_proto::KeysEnv {
        let v = |seg: &str| Self::module_value(lane, index, &format!("{prefix}.{seg}"));
        signal_keys_proto::KeysEnv {
            attack_ms: v("attack"),
            decay_ms: v("decay"),
            sustain: v("sustain"),
            release_ms: v("release"),
        }
    }

    /// Every `(lane, module)` a scope's Global Controls reach. Same rule at
    /// both levels — a module that is off, empty or fully down is not driven.
    /// Mute is a performance state, not a patch state, so a muted lane still
    /// follows the engine (turn the engine back up and the patch is coherent).
    /// Whether this lane is kept out of the engine + rig Global Controls.
    fn lane_excluded(s: &State, layer: &str) -> bool {
        s.profile
            .engines
            .iter()
            .flat_map(|e| e.layers.iter())
            .find(|l| l.name == layer)
            .map(|l| l.exclude_global)
            .unwrap_or(false)
    }

    fn scope_targets(s: &State, lanes: &[String]) -> Vec<(String, usize)> {
        lanes
            .iter()
            // A lane can be kept out of the globals entirely — the piano you
            // don't want a filter sweep landing on. It keeps its own macros.
            .filter(|name| !Self::lane_excluded(s, name))
            .filter_map(|name| s.lanes.get(name).map(|lane| (name, lane)))
            .flat_map(|(name, lane)| {
                (0..lane.modules.len())
                    .filter(|i| lane.module_gain(*i) > 0.0)
                    .map(move |i| (name.clone(), i))
            })
            .collect()
    }

    /// One engine's lanes, in profile order.
    fn engine_lanes(s: &State, engine: &str) -> Vec<String> {
        s.profile
            .engines
            .iter()
            .filter(|e| e.name == engine)
            .flat_map(|e| e.layers.iter().map(|l| l.name.clone()))
            .collect()
    }

    /// Every lane in the profile, in profile order — the rig level's scope.
    fn all_lanes(s: &State) -> Vec<String> {
        s.profile
            .engines
            .iter()
            .flat_map(|e| e.layers.iter().map(|l| l.name.clone()))
            .collect()
    }

    /// Every engine name, in profile order.
    fn all_engines(s: &State) -> Vec<String> {
        s.profile.engines.iter().map(|e| e.name.clone()).collect()
    }

    /// A scope's Global Controls as the UI sees them: absolute and 1:1 while
    /// every module beneath agrees, a bipolar offset once they don't.
    ///
    /// `prefix` picks the level ([`LAYER`] / [`ENGINE`]); `globals` and
    /// `spans` are that scope's own maps.
    fn global_models(
        s: &State,
        prefix: &str,
        targets: &[(String, usize)],
        globals: &BTreeMap<String, f32>,
        spans: &BTreeMap<String, (f32, Baseline)>,
    ) -> Vec<KeysMacro> {
        GLOBALS
            .iter()
            .map(|def| {
                let id = format!("{prefix}{}", def.key);
                let Some(target) = def.target else {
                    // The scope's own parameter — always absolute.
                    let value = globals.get(&id).copied().unwrap_or(def.default);
                    return KeysMacro {
                        id,
                        name: def.name.to_string(),
                        group: def.group.to_string(),
                        value,
                        min: def.min,
                        max: def.max,
                        unit: def.unit.to_string(),
                        live: def.live,
                        bipolar: false,
                        spread: String::new(),
                    };
                };
                let values: Vec<f32> = targets
                    .iter()
                    .filter_map(|(lane, i)| {
                        s.lanes.get(lane).map(|l| Self::module_value(l, *i, target))
                    })
                    .collect();
                let lo = values.iter().copied().fold(f32::INFINITY, f32::min);
                let hi = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                // Once a knob is off centre it STAYS an offset. Sweeping to
                // an extreme can make the modules read alike for a moment;
                // that must not throw away what they came from.
                let agree = !spans.contains_key(&id)
                    && (values.is_empty() || (hi - lo).abs() <= (def.max - def.min) * 1e-4);
                if agree {
                    let value = values.first().copied().unwrap_or(def.default);
                    KeysMacro {
                        id,
                        name: def.name.to_string(),
                        group: def.group.to_string(),
                        value,
                        min: def.min,
                        max: def.max,
                        unit: def.unit.to_string(),
                        live: def.live && !targets.is_empty(),
                        bipolar: false,
                        spread: fmt_value(value, def.unit),
                    }
                } else {
                    let offset = spans.get(&id).map(|(o, _)| *o).unwrap_or(0.0);
                    KeysMacro {
                        id,
                        name: def.name.to_string(),
                        group: def.group.to_string(),
                        value: offset,
                        min: -1.0,
                        max: 1.0,
                        unit: String::new(),
                        live: def.live,
                        bipolar: true,
                        spread: format!(
                            "{} – {}",
                            fmt_value(lo, def.unit),
                            fmt_value(hi, def.unit)
                        ),
                    }
                }
            })
            .collect()
    }

    /// The layer's Global Controls — its own modules, under the `l.` prefix.
    fn layer_macro_models(s: &State, layer: &str) -> Vec<KeysMacro> {
        let Some(lane) = s.lanes.get(layer) else {
            return Vec::new();
        };
        let targets = Self::scope_targets(s, &[layer.to_string()]);
        Self::global_models(s, LAYER, &targets, &lane.globals, &lane.spans)
    }

    /// Move a Global Control over `targets` — the shared body of
    /// `set_layer_global` and `set_engine_global`.
    ///
    /// Absolute while the modules agree: the knob writes that value straight
    /// through, exactly like editing each module by hand. Once they differ it
    /// is a bipolar offset that scales every module from the settings it had
    /// when the knob left centre, toward the parameter's floor (−1) or
    /// ceiling (+1). Returns the span to remember — `None` once the knob is
    /// back at its detent, where the patch's own values are untouched.
    fn drive_global(
        s: &mut State,
        def: &GlobalDef,
        target: &str,
        targets: &[(String, usize)],
        span: Option<(f32, Baseline)>,
        value: f32,
    ) -> Option<(f32, Baseline)> {
        let values: Vec<f32> = targets
            .iter()
            .filter_map(|(lane, i)| s.lanes.get(lane).map(|l| Self::module_value(l, *i, target)))
            .collect();
        let lo = values.iter().copied().fold(f32::INFINITY, f32::min);
        let hi = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        // A knob that is already an offset keeps being one — see the read
        // model.
        let agree =
            span.is_none() && (values.is_empty() || (hi - lo).abs() <= (def.max - def.min) * 1e-4);
        let tdef = macro_def(target);
        let (tmin, tmax) = tdef.map(|d| (d.min, d.max)).unwrap_or((def.min, def.max));
        let write = |s: &mut State, lane: &str, index: usize, v: f32| {
            if let Some(m) = s.lanes.get_mut(lane).and_then(|l| l.modules.get_mut(index)) {
                m.macros.insert(target.to_string(), v.clamp(tmin, tmax));
            }
        };
        if agree {
            // 1:1 — and the span is meaningless now, so drop it.
            let v = value.clamp(tmin, tmax);
            for (lane, i) in targets {
                write(s, lane, *i, v);
            }
            return None;
        }
        let offset = value.clamp(-1.0, 1.0);
        // The baseline is captured once, when the knob leaves centre, and
        // reused until it comes back.
        let base: Baseline = match &span {
            Some((_, base)) => base.clone(),
            None => targets
                .iter()
                .filter_map(|(lane, i)| {
                    s.lanes
                        .get(lane)
                        .map(|l| (lane.clone(), *i, Self::module_value(l, *i, target)))
                })
                .collect(),
        };
        // The offset moves the whole GROUP, keeping the modules' relationship
        // intact: the module nearest the edge reaches it at ±100%, and every
        // other module travels by the same ratio (frequencies, times) or the
        // same amount (everything else). Nothing converges, so sweeping out
        // and back is lossless.
        let ratio = matches!(tdef.map(|d| d.unit), Some("Hz") | Some("ms"));
        let k = offset.abs();
        let edge = if offset >= 0.0 { tmax } else { tmin };
        let leader = base
            .iter()
            .map(|(_, _, b)| *b)
            .fold(None::<f32>, |acc, b| {
                Some(match acc {
                    None => b,
                    // Whichever module would hit the edge first.
                    Some(a) => {
                        if offset >= 0.0 {
                            a.max(b)
                        } else {
                            a.min(b)
                        }
                    }
                })
            })
            .unwrap_or(0.0);
        let scale = if ratio && leader > 0.0 && edge > 0.0 {
            (edge / leader).powf(k)
        } else {
            1.0
        };
        let shift = (edge - leader) * k;
        for (lane, i, b) in base.clone() {
            let v = if ratio && leader > 0.0 && edge > 0.0 {
                b * scale
            } else {
                b + shift
            };
            write(s, &lane, i, v);
        }
        (offset.abs() >= 1e-4).then_some((offset, base))
    }

    /// What a Global Control move needs afterwards: a program build when the
    /// voice count changed, otherwise just the live cells and a publish.
    /// Whether this macro is baked into the program and so needs a rebuild
    /// to be heard: the filter's settings, the envelopes' times, unison.
    fn macro_is_dsp(id: &str) -> bool {
        id.starts_with("filter.")
            || id.starts_with("env1.")
            || id.starts_with("env2.")
            || id.starts_with("vib.")
            || id == "source.unison"
            || id == "source.detune"
    }

    /// One module's DSP values, read from live state (own macros, else the
    /// macro defaults) — everything [`push_module_dsp`](Self::push_module_dsp)
    /// writes into the running engine.
    fn module_dsp_values(&self, layer: &str, module: usize) -> Option<[f32; 15]> {
        let s = self.inner.state.lock().ok()?;
        let lane = s.lanes.get(layer)?;
        lane.modules.get(module)?;
        let v = |id: &str| Self::module_value(lane, module, id);
        Some([
            v("filter.cutoff"),
            v("filter.reso"),
            v("filter.env_amt"),
            v("env1.attack"),
            v("env1.decay"),
            v("env1.sustain"),
            v("env1.release"),
            v("env2.attack"),
            v("env2.decay"),
            v("env2.sustain"),
            v("env2.release"),
            v("source.unison"),
            v("source.detune"),
            v("vib.rate"),
            v("vib.depth"),
        ])
    }

    /// Push one module's DSP macros — filter cutoff/resonance, the Amp and
    /// Filter envelope ADSRs, the filter-env amount, unison — into the
    /// RUNNING engine, live. This replaced the rebuild for these edits: the
    /// filter and env-amount land in the render tree's parameter overlay,
    /// the envelopes mutate their control sources in place, and the sampler
    /// source takes its own unison / attack / release setters. Returns
    /// `false` when nothing could be applied (engine stopped, lane not
    /// hosted) — the caller falls back to the rebuild path.
    fn push_module_dsp(&self, layer: &str, module_idx: usize) -> bool {
        use signal_sampler::native::AdsrParams;
        let Some(vals) = self.module_dsp_values(layer, module_idx) else {
            return false;
        };
        let [cutoff_hz, reso, env_amt, a1, d1, s1, r1, a2, d2, s2, r2, unison, detune, vib_rate, vib_depth] =
            vals;
        let module = format!("{layer} {}", signal_synth::engine::module_slot(module_idx));
        let Ok(rig) = self.inner.rig.lock() else {
            return false;
        };
        let Some(rig) = rig.as_ref() else {
            return false;
        };
        let sample_rate = rig.sample_rate().max(1);
        rig.edit_lane(layer, |inst| {
            let render = inst.render_mut();
            // Filter block params (normalized on the block's own scale).
            let cutoff_norm =
                signal_sampler::native::NativeFilter::norm_from_cutoff(cutoff_hz) as f64;
            let mut applied = render.set_leaf_param(&module, "Filter 1", "cutoff", cutoff_norm);
            applied |= render.set_leaf_param(
                &module,
                "Filter 1",
                "resonance",
                reso.clamp(0.0, 1.0) as f64,
            );
            // Envelope sources + the env→cutoff amount (synth modules; a
            // sampler module has no env sources — its voices carry their
            // own envelope, updated below).
            let adsr = |a: f32, d: f32, sus: f32, r: f32| AdsrParams {
                attack_s: (a / 1000.0).max(0.0),
                decay_s: (d / 1000.0).max(0.0),
                sustain: sus.clamp(0.0, 1.0),
                release_s: (r / 1000.0).max(0.0),
            };
            applied |= render.set_env(&module, "Amp Env", adsr(a1, d1, s1, r1));
            applied |= render.set_env(&module, "Filter Env", adsr(a2, d2, s2, r2));
            applied |= render.set_route_depth(
                &module,
                "Filter Env",
                "Filter 1",
                "cutoff",
                env_amt.clamp(-1.0, 1.0),
            );
            // Per-voice synth engine (the oscillator source): full amp +
            // filter ADSRs, its own lowpass, vibrato. Times map onto the
            // voice's 8 s segment range, normalized.
            let seg = |ms: f32| ((ms.max(0.0) / 1000.0) / 8.0).clamp(0.0, 1.0) as f64;
            applied |= render.set_leaf_param(&module, "Soundsource", "amp_attack", seg(a1));
            render.set_leaf_param(&module, "Soundsource", "amp_decay", seg(d1));
            render.set_leaf_param(
                &module,
                "Soundsource",
                "amp_sustain",
                s1.clamp(0.0, 1.0) as f64,
            );
            render.set_leaf_param(&module, "Soundsource", "amp_release", seg(r1));
            render.set_leaf_param(&module, "Soundsource", "filter_attack", seg(a2));
            render.set_leaf_param(&module, "Soundsource", "filter_decay", seg(d2));
            render.set_leaf_param(
                &module,
                "Soundsource",
                "filter_sustain",
                s2.clamp(0.0, 1.0) as f64,
            );
            render.set_leaf_param(&module, "Soundsource", "filter_release", seg(r2));
            render.set_leaf_param(&module, "Soundsource", "cutoff", cutoff_norm);
            render.set_leaf_param(
                &module,
                "Soundsource",
                "resonance",
                reso.clamp(0.0, 1.0) as f64,
            );
            render.set_leaf_param(
                &module,
                "Soundsource",
                "env_amt",
                ((env_amt.clamp(-1.0, 1.0) + 1.0) / 2.0) as f64,
            );
            render.set_leaf_param(
                &module,
                "Soundsource",
                "vib_rate",
                (vib_rate.clamp(0.1, 12.0) / 12.0) as f64,
            );
            render.set_leaf_param(
                &module,
                "Soundsource",
                "vib_depth",
                vib_depth.clamp(0.0, 1.0) as f64,
            );

            // Sampler source: per-voice amp attack/release + unison.
            applied |= render.with_sampler_source(&module, "Soundsource", |sampler| {
                let engine = sampler.engine_mut();
                engine.set_attack_frames(((a1.max(0.0) / 1000.0) * sample_rate as f32) as usize);
                engine.set_release_frames(((r1.max(0.0) / 1000.0) * sample_rate as f32) as usize);
                engine.set_unison(
                    (unison.max(1.0).round() as u8).min(8),
                    detune.clamp(0.0, 2.0) * 100.0,
                    0.7,
                );
            });
            applied
        })
        .unwrap_or(false)
    }

    /// Live-push every `(layer, module)` target; `true` only if every one
    /// applied (any miss → the caller rebuilds, which covers them all).
    fn push_targets_dsp(&self, targets: &[(String, usize)]) -> bool {
        !targets.is_empty()
            && targets
                .iter()
                .all(|(layer, i)| self.push_module_dsp(layer, *i))
    }

    /// Rebuild the program shortly, once the knob stops moving.
    ///
    /// Filter and envelope values are block params, so hearing them means
    /// rebuilding — and a rebuild re-opens the lane's packs. Rebuilding per
    /// drag event would make a knob sweep unusable, so each edit bumps a
    /// generation and only the last one standing does the work.
    fn rebuild_soon(&self) {
        use std::sync::atomic::Ordering;
        let gen = self.inner.rebuild_gen.fetch_add(1, Ordering::AcqRel) + 1;
        let b = self.clone();
        let _ = std::thread::Builder::new()
            .name("keys-param-rebuild".into())
            .spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(180));
                if b.inner.rebuild_gen.load(Ordering::Acquire) != gen {
                    return; // a later edit owns the rebuild
                }
                let _rt = keys_runtime().enter();
                b.rebuild_program();
            });
    }

    fn after_global(&self, rebuild: bool, targets: &[(String, usize)]) {
        // DSP parameters go LIVE into the running engine now (parameter
        // overlay + in-place envelope sources); the rebuild only remains as
        // the fallback when nothing is hosted yet.
        if rebuild && !self.push_targets_dsp(targets) {
            // Coalesced: a global sweep moves every lane's parameter at drag
            // rate, and each one is a program rebuild.
            self.rebuild_soon();
            self.publish_mixer();
        } else {
            self.apply_mixer();
            self.publish_mixer();
        }
    }

    /// Re-base every *other* Global Control that drives `target`: a hand edit
    /// — or a Global at another level — leaves their baselines describing a
    /// patch that no longer exists. `skip` is the id doing the driving, and
    /// the rig level is always re-based (it sits above everything).
    fn rebase_others(
        s: &mut State,
        engines: &[String],
        lanes: &[String],
        target: &str,
        skip: &str,
    ) {
        for g in GLOBALS.iter().filter(|g| g.target == Some(target)) {
            let (rig_id, engine_id, layer_id) = (
                format!("{RIG}{}", g.key),
                format!("{ENGINE}{}", g.key),
                format!("{LAYER}{}", g.key),
            );
            if rig_id != skip {
                s.rig_spans.remove(&rig_id);
            }
            if engine_id != skip {
                for name in engines {
                    if let Some(e) = s.engines.get_mut(name) {
                        e.spans.remove(&engine_id);
                    }
                }
            }
            if layer_id != skip {
                for name in lanes {
                    if let Some(lane) = s.lanes.get_mut(name) {
                        lane.spans.remove(&layer_id);
                    }
                }
            }
        }
    }

    /// Build the backend and scan the Keyscape library. Does not open audio.
    pub fn new() -> Self {
        let (presets, specs) = prefer_packs_from_scan();
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
            default = default_idx
                .and_then(|i| presets.get(i))
                .map(|p| p.name.as_str())
                .unwrap_or("<first>"),
            "keys rig: scanned library"
        );
        // The rig boots as a PROFILE — engines, layers, stacks — not a single
        // patch. `FTS_KEYS_PROFILE` points at a `.styx` profile; the built-in
        // Worship profile is the default.
        let profile = load_profile();
        let mut state = State {
            presets,
            specs,
            loaded: default_idx,
            ..State::default()
        };
        state.adopt_profile(profile);
        // Lanes whose authored patch isn't in the library start empty rather
        // than silently pointing at a missing pack.
        //
        // Matched with the same normalization the soundsource index uses, and
        // rewritten to the library's own spelling on a match: a profile names
        // a patch the way the patch does (`Dolceola ^ RR Lite`) while the
        // library names it the way the extraction wrote the folder
        // (`Dolceola`). Two lookups disagreeing is exactly how a lane ends up
        // quietly empty, which is what this loop exists to prevent.
        // A `.signalpack` is preferred over a `.prt_omn` of the same name, and
        // resolution is tried against packs FIRST.
        //
        // Both can carry one name — "Choir Men Ohs" is a built pack and an
        // Omnisphere factory patch — and only the pack is loadable today: the
        // sampler's spec loader cannot parse a patch file, so picking the
        // patch fails the lane outright with
        // `spec parse error: unexpected token`. Preferring packs is also just
        // right on the merits; a pack is cheaper to play than a patch tree.
        let pack_names: Vec<String> = state
            .presets
            .iter()
            .zip(state.specs.iter())
            .filter(|(_, spec)| {
                spec.extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("signalpack"))
            })
            .map(|(p, _)| p.name.clone())
            .collect();
        let known: Vec<String> = state.presets.iter().map(|p| p.name.clone()).collect();
        let canonical = |want: &str| -> Option<String> {
            use signal_synth::omni_import::resolve_name;
            resolve_name(want, pack_names.iter().map(String::as_str))
                .or_else(|| resolve_name(want, known.iter().map(String::as_str)))
                .map(str::to_string)
        };
        for lane in state.lanes.values_mut() {
            for m in lane.modules.iter_mut() {
                if m.patch.is_empty() {
                    continue;
                }
                match canonical(&m.patch) {
                    Some(name) => m.patch = name,
                    None => {
                        tracing::info!(patch = %m.patch, "keys rig: profile patch not in library — module starts empty");
                        m.patch.clear();
                    }
                }
            }
        }
        let backend = Self {
            inner: Arc::new(Inner {
                rig: Mutex::new(None),
                state: Mutex::new(state),
                events: architect::rig::events_hub(),
                pump_started: AtomicBool::new(false),
                rebuild_gen: std::sync::atomic::AtomicU64::new(0),
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

    /// The live profile + patch-spec index + lane snapshot behind both
    /// program builders (single tree and lane program).
    fn profile_inputs(&self) -> Option<ProfileInputs> {
        let s = self.inner.state.lock().ok()?;
        if s.profile.engines.is_empty() {
            return None;
        }
        // Patch name → spec path, via the scanned library.
        let index: BTreeMap<String, PathBuf> = s
            .presets
            .iter()
            .zip(s.specs.iter())
            .map(|(p, spec)| (p.name.clone(), spec.clone()))
            .collect();
        // Lanes carry the LIVE patch assignment (a stack recall or a browser
        // pick), which is what should be rendering — not the authored default.
        let mut profile = s.profile.clone();
        for engine in &mut profile.engines {
            for layer in &mut engine.layers {
                let Some(lane) = s.lanes.get(&layer.name) else {
                    continue;
                };
                let patches: Vec<String> = lane.modules.iter().map(|m| m.patch.clone()).collect();
                layer.patch = patches.first().cloned().unwrap_or_default();
                layer.extra_modules = patches.into_iter().skip(1).collect();
            }
        }
        Some((profile, index, s.lanes.clone()))
    }

    /// Each module's macros ride into the tree: the Filter block gets its
    /// cutoff and resonance, the Amp Env and Filter Env their times. This
    /// is what makes those blocks sound rather than sit there.
    fn module_settings_for(
        lanes: &BTreeMap<String, LaneState>,
        layer: &str,
        module: usize,
    ) -> signal_synth::engine::ModuleSettings {
        let mut set = signal_synth::engine::ModuleSettings::default();
        let Some(lane) = lanes.get(layer) else {
            return set;
        };
        let v = |id: &str| {
            lane.modules
                .get(module)
                .and_then(|m| m.macros.get(id).copied())
                .or_else(|| macro_def(id).map(|d| d.default))
                .unwrap_or(0.0)
        };
        set.cutoff_hz = v("filter.cutoff");
        set.resonance = v("filter.reso");
        set.filter_env_depth = v("filter.env_amt");
        set.amp_env = (
            v("env1.attack"),
            v("env1.decay"),
            v("env1.sustain"),
            v("env1.release"),
        );
        set.filter_env = (
            v("env2.attack"),
            v("env2.decay"),
            v("env2.sustain"),
            v("env2.release"),
        );
        set.unison = v("source.unison").max(1.0) as u32;
        set.detune = v("source.detune");
        set
    }

    /// Build the profile's full program: every engine, every layer, each
    /// lane's patch resolved to its pack/library spec.
    fn profile_program(&self) -> Option<Container> {
        let (profile, index, lanes) = self.profile_inputs()?;
        Some(profile.build_tree_with(
            |patch| index.get(patch).map(|p| p.to_string_lossy().into_owned()),
            |layer, module| Self::module_settings_for(&lanes, layer, module),
        ))
    }

    /// Build the profile as a per-lane daw-track program — the fully
    /// daw-based mixer (each layer/engine a real track).
    fn profile_lane_program(&self) -> Option<signal_sampler::keys_rig::LaneProgram> {
        let (profile, index, lanes) = self.profile_inputs()?;
        Some(profile.build_lane_program(
            |patch| index.get(patch).map(|p| p.to_string_lossy().into_owned()),
            |layer, module| Self::module_settings_for(&lanes, layer, module),
        ))
    }

    /// Push every live fader / mute / solo into the running program.
    ///
    /// Lane mode: faders/mutes/solos are daw track ops (the renderer folds
    /// solo-exclusion and folder mutes natively); only module trims stay
    /// in-tree cells. Single mode: everything is cells, with the mute/solo
    /// fold computed here. Both are safe at UI drag rate.
    fn apply_mixer(&self) {
        let Ok(rig) = self.inner.rig.lock() else {
            return;
        };
        let Some(rig) = rig.as_ref() else { return };
        let Ok(s) = self.inner.state.lock() else {
            return;
        };
        if rig.is_lanes() {
            for (name, lane) in s.lanes.iter() {
                rig.set_lane_volume(Role::Layer, name, db_to_linear(lane.gain_db));
                rig.set_lane_mute(Role::Layer, name, lane.muted);
                rig.set_lane_solo(Role::Layer, name, lane.soloed);
                if let Some(cells) = rig.lane_cells(name) {
                    // Modules are named "<layer> <slot>" in the lane's tree.
                    for i in 0..lane.modules.len() {
                        let module_name =
                            format!("{name} {}", signal_synth::engine::module_slot(i));
                        cells.set(Role::Module, &module_name, lane.module_gain(i));
                    }
                }
            }
            for (name, e) in s.engines.iter() {
                rig.set_lane_volume(Role::Engine, name, db_to_linear(e.gain_db));
                rig.set_lane_mute(Role::Engine, name, e.muted);
            }
        } else {
            let cells = rig.gain_cells();
            // Address by (role, name), never name alone: a one-lane engine is
            // named after its lane ("Pad" holding "Pad"), and a flat name map
            // gave both the same cell — the engine's trim, written second,
            // put the lane's mute and solo straight back.
            for (name, lane) in s.lanes.iter() {
                cells.set(Role::Layer, name, s.lane_gain(name));
                // Modules are named "<layer> <slot>" in the tree.
                for i in 0..lane.modules.len() {
                    let module_name = format!("{name} {}", signal_synth::engine::module_slot(i));
                    cells.set(Role::Module, &module_name, lane.module_gain(i));
                }
            }
            for name in s.engines.keys() {
                cells.set(Role::Engine, name, s.engine_gain(name));
            }
        }
        rig.set_output_gain(db_to_linear(s.master_db));
    }

    /// Rebuild the playable program from the profile (patch assignment
    /// changed), then re-apply the mixer to the fresh program.
    fn rebuild_program(&self) {
        // Breadcrumbs: a rebuild touches the state lock, the rig lock and the
        // audio device in that order, so a stall is only diagnosable if the
        // log says which step it reached.
        tracing::debug!("keys rig: rebuild — building program");
        // The single tree is always built: it is the control-view structure
        // (`s.tree`) even when the audio is hosted per lane.
        let Some(tree) = self.profile_program() else {
            return;
        };
        tracing::debug!("keys rig: rebuild — ensuring audio");
        if !self.ensure_open() {
            self.publish_all();
            return;
        }
        tracing::debug!("keys rig: rebuild — loading program");
        let lane_program = self
            .inner
            .rig
            .lock()
            .ok()
            .and_then(|r| r.as_ref().map(|r| r.is_lanes()))
            .unwrap_or(false)
            .then(|| self.profile_lane_program())
            .flatten();
        if let Ok(mut rig) = self.inner.rig.lock() {
            if let Some(rig) = rig.as_mut() {
                match lane_program {
                    Some(prog) => {
                        if let Err(e) = rig.load_lanes(&prog) {
                            tracing::error!("keys rig: lane reload failed: {e}");
                        }
                    }
                    None => rig.load_preset(&tree),
                }
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
            let Ok(mut s) = self.inner.state.lock() else {
                return;
            };
            // Soundsource names resolve against the same library the modules
            // load from; an unknown one leaves that module empty.
            let known: Vec<String> = s.presets.iter().map(|p| p.name.clone()).collect();
            let Some(lane) = s.lanes.get_mut(layer) else {
                return;
            };
            // Opening a preset names the LANE — the modules keep their own
            // soundsource names.
            if start == 0 {
                lane.preset = imported.name.clone();
            }
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
            // The lane holds exactly the modules the preset uses — an
            // unused slot is nothing, and more can be added any time.
            if start == 0 {
                lane.modules.truncate(imported.modules.len().max(1));
                lane.spans.clear();
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
        let Ok(s) = self.inner.state.lock() else {
            return KeysMixer::default();
        };
        let engines = s
            .profile
            .engines
            .iter()
            .map(|engine| {
                let est = s.engines.get(&engine.name).cloned().unwrap_or_default();
                KeysEngineModel {
                    name: engine.name.clone(),
                    // A drone engine carries its key selector; the card
                    // embeds it instead of the usual played-engine chrome.
                    drone: est.drone.clone().or_else(|| {
                        is_drone(&engine.name).then_some(signal_keys_proto::KeysDrone {
                            key: 0,
                            playing: false,
                            octave: 3,
                        })
                    }),
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
                                preset: lane.map(|l| l.preset.clone()).unwrap_or_default(),
                                gain_db: lane.map(|l| l.gain_db).unwrap_or(0.0),
                                muted: lane.is_some_and(|l| l.muted),
                                exclude_global: layer.exclude_global,
                                soloed: lane.is_some_and(|l| l.soloed),
                                live: lane.is_some_and(|l| l.any_live()),
                                key_lo: layer.key_lo as u32,
                                key_hi: layer.key_hi as u32,
                                modules: lane
                                    .map(|lane| {
                                        lane.modules
                                            .iter()
                                            .enumerate()
                                            .map(|(i, m)| signal_keys_proto::KeysModule {
                                                index: i as u32,
                                                slot: signal_synth::engine::module_slot(i),
                                                patch: m.patch.clone(),
                                                variant: m.variant.clone(),
                                                live: !m.patch.is_empty(),
                                                gain_db: m.gain_db,
                                                enabled: m.enabled,
                                                amp_env: Self::module_env(lane, i, "env1"),
                                                filter_env: Self::module_env(lane, i, "env2"),
                                                cutoff_hz: Self::module_value(
                                                    lane,
                                                    i,
                                                    "filter.cutoff",
                                                ),
                                                resonance: Self::module_value(
                                                    lane,
                                                    i,
                                                    "filter.reso",
                                                ),
                                                dly_time_ms: Self::module_value(
                                                    lane, i, "dly.time",
                                                ),
                                                dly_div: Self::module_value(lane, i, "dly.div"),
                                                dly_feedback: Self::module_value(
                                                    lane,
                                                    i,
                                                    "dly.feedback",
                                                ),
                                                dly_mix: Self::module_value(lane, i, "dly.mix"),
                                                amb_size: Self::module_value(lane, i, "amb.size"),
                                                amb_mix: Self::module_value(lane, i, "amb.mix"),
                                                amb_predelay_ms: Self::module_value(
                                                    lane,
                                                    i,
                                                    "amb.predelay",
                                                ),
                                                amb_decay: Self::module_value(lane, i, "amb.decay"),
                                                unison: Self::module_value(
                                                    lane,
                                                    i,
                                                    "source.unison",
                                                ),
                                                detune: Self::module_value(
                                                    lane,
                                                    i,
                                                    "source.detune",
                                                ),
                                                vib_rate: Self::module_value(lane, i, "vib.rate"),
                                                vib_depth: Self::module_value(lane, i, "vib.depth"),
                                                vib_delay_ms: Self::module_value(
                                                    lane,
                                                    i,
                                                    "vib.delay",
                                                ),
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
        KeysMixer {
            profile: s.profile.name.clone(),
            engines,
            master_db: s.master_db,
        }
    }

    /// The wire performance snapshot.
    fn perform_model(&self) -> KeysPerform {
        let Ok(s) = self.inner.state.lock() else {
            return KeysPerform::default();
        };
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
        self.inner
            .events
            .publish(KeysEvent::Mixer(self.mixer_model()));
    }

    fn publish_perform(&self) {
        self.inner
            .events
            .publish(KeysEvent::Perform(self.perform_model()));
    }

    /// Open audio with the given (or first) preset, if not already open.
    fn ensure_open(&self) -> bool {
        {
            if self.inner.rig.lock().map(|r| r.is_some()).unwrap_or(false) {
                return true;
            }
        }
        // The profile IS the program — engines, layers, every lane's patch —
        // hosted as per-lane daw tracks (the fully daw-based mixer). A
        // profile-less build falls back to the single-patch, single-track
        // program, which is what the mobile shell's one-pack case wants.
        let idx = self
            .inner
            .state
            .lock()
            .ok()
            .and_then(|s| s.loaded)
            .unwrap_or(0);
        let lane_program = self.profile_lane_program();
        // The single tree doubles as the control-view structure either way.
        let tree = self.profile_program().or_else(|| self.program_for(idx));
        if lane_program.is_none() && tree.is_none() {
            if let Ok(mut s) = self.inner.state.lock() {
                s.last_error = Some("no patches downloaded yet".into());
            }
            return false;
        }
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
        self.inner
            .events
            .publish(KeysEvent::Status(KeysRigSvc::status(self)));
        let opened = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            match (&lane_program, &tree) {
                (Some(prog), _) => KeysRig::open_lanes(&prefs, prog),
                (None, Some(tree)) => KeysRig::open(&prefs, tree),
                (None, None) => unreachable!("guarded above"),
            }
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
                // A freshly opened rig has no MIDI input yet, and every
                // caller of ensure_open (start, preset load, rebuild) needs
                // one — attaching here makes this THE single open path, so a
                // rig can never end up audible-but-deaf.
                self.reattach_midi();
                if let Ok(mut s) = self.inner.state.lock() {
                    if s.loaded.is_none() {
                        s.loaded = Some(idx);
                    }
                    for (i, p) in s.presets.iter_mut().enumerate() {
                        p.loaded = i == idx;
                    }
                    s.tree = tree;
                    s.last_error = None;
                }
                true
            }
            Err(e) => {
                tracing::error!("keys rig: audio open failed: {e}");
                if let Ok(mut s) = self.inner.state.lock() {
                    s.last_error = Some(format!("audio open failed: {e}"));
                }
                self.inner
                    .events
                    .publish(KeysEvent::Status(KeysRigSvc::status(self)));
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
                if rig.is_lanes() {
                    // Whole-rig preset audition while hosted per lane: one
                    // engine, one lane, the preset's tree.
                    let prog = signal_sampler::keys_rig::LaneProgram {
                        name: tree.name.clone(),
                        engines: vec![signal_sampler::keys_rig::LaneEngine {
                            name: tree.name.clone(),
                            layers: vec![signal_sampler::keys_rig::LaneLayer {
                                name: tree.name.clone(),
                                tree: tree.clone(),
                            }],
                        }],
                        tail: None,
                    };
                    if let Err(e) = rig.load_lanes(&prog) {
                        tracing::error!("keys rig: preset audition reload failed: {e}");
                    }
                } else {
                    rig.load_preset(&tree);
                }
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
        let port = self
            .inner
            .state
            .lock()
            .ok()
            .and_then(|s| s.midi_port.clone());
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
        self.inner
            .events
            .publish(KeysEvent::Library(KeysRigSvc::presets(self)));
        self.inner
            .events
            .publish(KeysEvent::Tree(KeysRigSvc::tree(self)));
        self.inner
            .events
            .publish(KeysEvent::Status(KeysRigSvc::status(self)));
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
        self.inner
            .events
            .publish(KeysEvent::Status(KeysRigSvc::status(self)));
        self.inner
            .events
            .publish(KeysEvent::Tree(KeysRigSvc::tree(self)));
    }

    fn on_running_tick(&self) {
        self.inner
            .events
            .publish(KeysEvent::Status(KeysRigSvc::status(self)));
        self.inner
            .events
            .publish(KeysEvent::Midi(KeysRigSvc::midi_recent(self)));
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
        self.inner
            .events
            .publish(KeysEvent::Status(KeysRigSvc::status(self)));
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
                // ensure_open attaches MIDI itself — it is the single open
                // path for every entry point (start, preset load, rebuild).
                if b.ensure_open() {
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
        self.inner
            .events
            .publish(KeysEvent::Status(KeysRigSvc::status(self)));
    }

    fn status(&self) -> KeysStatus {
        tracing::debug!("keys rpc: status →");
        let running = self.inner.rig.lock().map(|r| r.is_some()).unwrap_or(false);
        let s = self.inner.state.lock().unwrap();
        let loaded_preset = s
            .loaded
            .and_then(|i| s.presets.get(i))
            .map(|p| p.name.clone());
        let (master_peak, meters) = if running {
            self.inner
                .rig
                .lock()
                .ok()
                .and_then(|r| {
                    r.as_ref().map(|r| {
                        let meters = r
                            .cell_peaks()
                            .into_iter()
                            .map(|(role, name, peak)| KeysMeter {
                                kind: role.tag().to_lowercase(),
                                name,
                                peak,
                            })
                            .collect();
                        (r.output_peak(), meters)
                    })
                })
                .unwrap_or_default()
        } else {
            (0.0, Vec::new())
        };
        KeysStatus {
            running,
            loaded_preset,
            master_peak,
            meters,
            voices: 0,
            midi_port: s.midi_port.clone(),
            last_error: s.last_error.clone(),
        }
    }

    fn presets(&self) -> Vec<KeysPreset> {
        tracing::debug!("keys rpc: presets →");
        self.inner
            .state
            .lock()
            .map(|s| s.presets.clone())
            .unwrap_or_default()
    }

    fn rescan(&self) {
        let (presets, specs) = prefer_packs_from_scan();
        if let Ok(mut s) = self.inner.state.lock() {
            // Keep the loaded preset marked if it survived the rescan.
            let loaded_name = s
                .loaded
                .and_then(|i| s.presets.get(i))
                .map(|p| p.name.clone());
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
        tracing::debug!("keys rpc: tree →");
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

    fn pitch_bend(&self, raw: u32) {
        if let Ok(rig) = self.inner.rig.lock() {
            if let Some(rig) = rig.as_ref() {
                rig.pitch_bend(raw.min(16_383) as u16);
            }
        }
    }

    fn mod_wheel(&self, value: u32) {
        if let Ok(rig) = self.inner.rig.lock() {
            if let Some(rig) = rig.as_ref() {
                rig.cc(1, value.min(127) as u8);
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
        self.inner
            .events
            .publish(KeysEvent::Status(KeysRigSvc::status(self)));
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
        tracing::debug!("keys rpc: mixer →");
        self.mixer_model()
    }

    fn set_layer_gain(&self, layer: String, db: f32) {
        if let Ok(mut s) = self.inner.state.lock() {
            let Some(lane) = s.lanes.get_mut(&layer) else {
                return;
            };
            lane.gain_db = db.clamp(MIN_FADER_DB, MAX_FADER_DB);
        }
        self.apply_mixer();
        self.publish_mixer();
    }

    fn set_engine_gain(&self, engine: String, db: f32) {
        if let Ok(mut s) = self.inner.state.lock() {
            let Some(e) = s.engines.get_mut(&engine) else {
                return;
            };
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
            let Some(lane) = s.lanes.get_mut(&layer) else {
                return;
            };
            lane.muted = muted;
        }
        self.apply_mixer();
        self.publish_mixer();
    }

    fn set_engine_mute(&self, engine: String, muted: bool) {
        if let Ok(mut s) = self.inner.state.lock() {
            let Some(e) = s.engines.get_mut(&engine) else {
                return;
            };
            e.muted = muted;
        }
        self.apply_mixer();
        self.publish_mixer();
    }

    fn set_layer_exclude_global(&self, layer: String, excluded: bool) {
        {
            let Ok(mut s) = self.inner.state.lock() else {
                return;
            };
            let Some(def) = s
                .profile
                .engines
                .iter_mut()
                .flat_map(|e| e.layers.iter_mut())
                .find(|l| l.name == layer)
            else {
                return;
            };
            if def.exclude_global == excluded {
                return;
            }
            def.exclude_global = excluded;
            // The globals' baselines were taken over a different set of
            // lanes; they have to be re-taken over the new one.
            for engine in Self::all_engines(&s) {
                if let Some(e) = s.engines.get_mut(&engine) {
                    e.spans.clear();
                }
            }
            s.rig_spans.clear();
        }
        if let Ok(s) = self.inner.state.lock() {
            s.profile.save();
        }
        self.publish_mixer();
    }

    fn set_layer_solo(&self, layer: String, soloed: bool) {
        if let Ok(mut s) = self.inner.state.lock() {
            let Some(lane) = s.lanes.get_mut(&layer) else {
                return;
            };
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
            .filter(|p| {
                p.extension()
                    .is_some_and(|x| x.eq_ignore_ascii_case("prt_omn"))
            });
        if let Some(file) = patch_file {
            self.load_omni_patch(&layer, module as usize, &file);
            return;
        }
        {
            let Ok(mut s) = self.inner.state.lock() else {
                return;
            };
            let Some(name) = s.presets.get(preset as usize).map(|p| p.name.clone()) else {
                return;
            };
            let Some(lane) = s.lanes.get_mut(&layer) else {
                return;
            };
            let Some(m) = lane.modules.get_mut(module as usize) else {
                return;
            };
            if m.patch == name {
                return;
            }
            m.patch = name;
            // A hand-picked source means the lane is no longer just the
            // preset it was opened from.
            lane.preset.clear();
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
        tracing::debug!(%layer, module, "keys rpc: layer_detail →");
        let Ok(s) = self.inner.state.lock() else {
            return KeysLayerDetail::default();
        };
        let Some(lane) = s.lanes.get(&layer) else {
            return KeysLayerDetail::default();
        };
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
                patch: m.patch.clone(),
                variant: m.variant.clone(),
                live: !m.patch.is_empty(),
                gain_db: m.gain_db,
                enabled: m.enabled,
                amp_env: Self::module_env(lane, i, "env1"),
                filter_env: Self::module_env(lane, i, "env2"),
                cutoff_hz: Self::module_value(lane, i, "filter.cutoff"),
                resonance: Self::module_value(lane, i, "filter.reso"),
                dly_time_ms: Self::module_value(lane, i, "dly.time"),
                dly_div: Self::module_value(lane, i, "dly.div"),
                dly_feedback: Self::module_value(lane, i, "dly.feedback"),
                dly_mix: Self::module_value(lane, i, "dly.mix"),
                amb_size: Self::module_value(lane, i, "amb.size"),
                amb_mix: Self::module_value(lane, i, "amb.mix"),
                amb_predelay_ms: Self::module_value(lane, i, "amb.predelay"),
                amb_decay: Self::module_value(lane, i, "amb.decay"),
                unison: Self::module_value(lane, i, "source.unison"),
                detune: Self::module_value(lane, i, "source.detune"),
                vib_rate: Self::module_value(lane, i, "vib.rate"),
                vib_depth: Self::module_value(lane, i, "vib.depth"),
                vib_delay_ms: Self::module_value(lane, i, "vib.delay"),
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
                bipolar: false,
                spread: String::new(),
            })
            .collect();
        // The selected MODULE's slice of the live program.
        let module_name = format!("{layer} {}", signal_synth::engine::module_slot(slot));
        let tree = s
            .tree
            .as_ref()
            .and_then(|t| t.find(&module_name).map(|c| node_of(c, "")))
            .unwrap_or_default();
        let layer_macros = Self::layer_macro_models(&s, &layer);
        KeysLayerDetail {
            layer: layer.clone(),
            engine: lane.engine.clone(),
            modules,
            module: slot as u32,
            patch: here.map(|m| m.patch.clone()).unwrap_or_default(),
            preset: lane.preset.clone(),
            gain_db: lane.gain_db,
            muted: lane.muted,
            key_lo,
            key_hi,
            macros,
            layer_macros,
            tree,
        }
    }

    /// Move a layer Global Control — see `KeysRigBackend::drive_global` for
    /// the absolute/offset rule.
    fn set_layer_global(&self, layer: String, id: String, value: f32) {
        let Some(def) = global_def(&id) else { return };
        let rebuild;
        let dsp_targets;
        {
            let Ok(mut s) = self.inner.state.lock() else {
                return;
            };
            let Some(lane) = s.lanes.get_mut(&layer) else {
                return;
            };
            let engine = lane.engine.clone();
            let Some(target) = def.target else {
                lane.globals.insert(id, value.clamp(def.min, def.max));
                drop(s);
                self.publish_mixer();
                return;
            };
            let targets = Self::scope_targets(&s, std::slice::from_ref(&layer));
            let span = s.lanes.get(&layer).and_then(|l| l.spans.get(&id).cloned());
            let next = Self::drive_global(&mut s, def, target, &targets, span, value);
            if let Some(lane) = s.lanes.get_mut(&layer) {
                match next {
                    Some(span) => lane.spans.insert(id.clone(), span),
                    None => lane.spans.remove(&id),
                };
            }
            // The engine's knob for the same parameter now describes a patch
            // that moved under it.
            Self::rebase_others(
                &mut s,
                std::slice::from_ref(&engine),
                std::slice::from_ref(&layer),
                target,
                &id,
            );
            rebuild = Self::macro_is_dsp(target);
            dsp_targets = targets;
        }
        self.after_global(rebuild, &dsp_targets);
    }

    fn engine_detail(&self, engine: String) -> KeysEngineDetail {
        tracing::debug!(%engine, "keys rpc: engine_detail →");
        let Ok(s) = self.inner.state.lock() else {
            return KeysEngineDetail::default();
        };
        let Some(est) = s.engines.get(&engine).cloned() else {
            return KeysEngineDetail::default();
        };
        let lanes = Self::engine_lanes(&s, &engine);
        let targets = Self::scope_targets(&s, &lanes);
        let live_layers = lanes
            .iter()
            .filter(|n| s.lanes.get(*n).is_some_and(|l| l.any_live() && !l.muted))
            .count() as u32;
        KeysEngineDetail {
            engine: engine.clone(),
            gain_db: est.gain_db,
            muted: est.muted,
            macros: Self::global_models(&s, ENGINE, &targets, &est.globals, &est.spans),
            live_layers,
            layers: lanes.len() as u32,
        }
    }

    /// Move an engine Global Control — the layer rule over every lane at once.
    fn set_engine_global(&self, engine: String, id: String, value: f32) {
        let Some(def) = global_def(&id) else { return };
        let rebuild;
        let dsp_targets;
        {
            let Ok(mut s) = self.inner.state.lock() else {
                return;
            };
            if !s.engines.contains_key(&engine) {
                return;
            }
            let Some(target) = def.target else {
                if let Some(e) = s.engines.get_mut(&engine) {
                    e.globals.insert(id, value.clamp(def.min, def.max));
                }
                drop(s);
                self.publish_mixer();
                return;
            };
            let lanes = Self::engine_lanes(&s, &engine);
            let targets = Self::scope_targets(&s, &lanes);
            let span = s
                .engines
                .get(&engine)
                .and_then(|e| e.spans.get(&id).cloned());
            let next = Self::drive_global(&mut s, def, target, &targets, span, value);
            if let Some(e) = s.engines.get_mut(&engine) {
                match next {
                    Some(span) => e.spans.insert(id.clone(), span),
                    None => e.spans.remove(&id),
                };
            }
            // Every lane knob for this parameter has been moved from under it.
            Self::rebase_others(&mut s, std::slice::from_ref(&engine), &lanes, target, &id);
            rebuild = Self::macro_is_dsp(target);
            dsp_targets = targets;
        }
        self.after_global(rebuild, &dsp_targets);
    }

    fn rig_macros(&self) -> Vec<KeysMacro> {
        let Ok(s) = self.inner.state.lock() else {
            return Vec::new();
        };
        let targets = Self::scope_targets(&s, &Self::all_lanes(&s));
        Self::global_models(&s, RIG, &targets, &s.rig_globals, &s.rig_spans)
    }

    /// Move a rig Global Control — the same rule over the whole profile.
    fn set_rig_global(&self, id: String, value: f32) {
        let Some(def) = global_def(&id) else { return };
        let rebuild;
        let dsp_targets;
        {
            let Ok(mut s) = self.inner.state.lock() else {
                return;
            };
            let Some(target) = def.target else {
                s.rig_globals.insert(id, value.clamp(def.min, def.max));
                drop(s);
                self.publish_mixer();
                return;
            };
            let lanes = Self::all_lanes(&s);
            let engines = Self::all_engines(&s);
            let targets = Self::scope_targets(&s, &lanes);
            let span = s.rig_spans.get(&id).cloned();
            let next = Self::drive_global(&mut s, def, target, &targets, span, value);
            match next {
                Some(span) => s.rig_spans.insert(id.clone(), span),
                None => s.rig_spans.remove(&id),
            };
            // Every engine and lane knob for this parameter has been moved
            // from under it.
            Self::rebase_others(&mut s, &engines, &lanes, target, &id);
            rebuild = Self::macro_is_dsp(target);
            dsp_targets = targets;
        }
        self.after_global(rebuild, &dsp_targets);
    }

    fn set_layer_macro(&self, layer: String, module: u32, id: String, value: f32) {
        let Some(def) = macro_def(&id) else { return };
        if let Ok(mut s) = self.inner.state.lock() {
            let Some(lane) = s.lanes.get(&layer) else {
                return;
            };
            let engine = lane.engine.clone();
            // A hand edit re-bases every Global Control above it that drives
            // this parameter — their baselines described a patch that is gone.
            Self::rebase_others(
                &mut s,
                std::slice::from_ref(&engine),
                std::slice::from_ref(&layer),
                def.id,
                "",
            );
            let Some(m) = s
                .lanes
                .get_mut(&layer)
                .and_then(|l| l.modules.get_mut(module as usize))
            else {
                return;
            };
            m.macros.insert(id, value.clamp(def.min, def.max));
        }
        // `source.level` rides the lane fader, which is a live cell.
        if def.id == "source.level" {
            self.apply_mixer();
        }
        // DSP macros go live into the running engine (filter / envelopes /
        // unison); the rebuild only remains as the not-yet-hosted fallback.
        if Self::macro_is_dsp(def.id) && !self.push_module_dsp(&layer, module as usize) {
            self.rebuild_soon();
        }
        self.publish_mixer();
    }

    fn set_drone(&self, engine: String, key: u32, octave: i32, playing: bool) {
        if let Ok(mut s) = self.inner.state.lock() {
            let est = s.engines.entry(engine.clone()).or_default();
            let mut d = est.drone.clone().unwrap_or_default();
            d.key = key.min(11);
            d.playing = playing;
            // Under the band, not in it — and never so low it is mud or so
            // high it is a part.
            d.octave = octave.clamp(1, 5);
            est.drone = Some(d);
        }
        // Nothing sounds yet — the drone's voice is not wired to the engine,
        // so this is the state the note will be driven from.
        self.publish_mixer();
    }

    fn set_engine_order(&self, engines: Vec<String>) {
        if let Ok(mut s) = self.inner.state.lock() {
            // Rank by the requested order; anything unnamed keeps its place
            // behind the named ones, so a caller can promote one engine
            // without having to restate the whole mixer.
            s.profile.apply_order(&engines);
            // The order belongs to the profile, so it is written with it —
            // a mixer the player rearranged comes back rearranged.
            s.profile.save();
        }
        // Engines sum in parallel, so order is presentation only — the tree
        // does not need rebuilding and nothing stops sounding.
        self.publish_mixer();
    }

    fn set_layer_variant(&self, layer: String, module: u32, preset: u32, variant: u32) {
        // A variation shares its default's soundsource, so the load is the
        // ordinary one — what differs is which variation the module records.
        // Authored parameter sets apply here, over the loaded default, once
        // packs carry them (see `crate::variations`).
        let name = self
            .inner
            .state
            .lock()
            .ok()
            .and_then(|s| s.presets.get(preset as usize).cloned())
            .map(|p| p.name)
            .unwrap_or_default();
        let chosen = crate::variations::variations_for(&name)
            .get(variant as usize)
            .map(|v| v.name.to_string())
            .unwrap_or_default();
        KeysRigSvc::set_layer_patch(self, layer.clone(), module, preset);
        if let Ok(mut s) = self.inner.state.lock() {
            if let Some(lane) = s.lanes.get_mut(&layer) {
                if let Some(m) = lane.modules.get_mut(module as usize) {
                    m.variant = chosen;
                }
            }
        }
        self.publish_mixer();
    }

    fn clear_layer(&self, layer: String, module: u32) {
        {
            let Ok(mut s) = self.inner.state.lock() else {
                return;
            };
            let Some(lane) = s.lanes.get_mut(&layer) else {
                return;
            };
            let Some(m) = lane.modules.get_mut(module as usize) else {
                return;
            };
            if m.patch.is_empty() {
                return;
            }
            m.patch.clear();
            lane.preset.clear();
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
            let Some(lane) = s.lanes.get_mut(&layer) else {
                return;
            };
            let Some(m) = lane.modules.get_mut(module as usize) else {
                return;
            };
            m.gain_db = db.clamp(MIN_FADER_DB, MAX_FADER_DB);
        }
        self.apply_mixer();
        self.publish_mixer();
    }

    fn set_module_enabled(&self, layer: String, module: u32, on: bool) {
        if let Ok(mut s) = self.inner.state.lock() {
            let Some(lane) = s.lanes.get_mut(&layer) else {
                return;
            };
            let Some(m) = lane.modules.get_mut(module as usize) else {
                return;
            };
            m.enabled = on;
        }
        self.apply_mixer();
        self.publish_mixer();
    }

    // ── Performance ──────────────────────────────────────────────────────

    fn perform(&self) -> KeysPerform {
        tracing::debug!("keys rpc: perform →");
        self.perform_model()
    }

    fn press_stack(&self, index: u32) {
        let needs_rebuild = {
            let Ok(mut s) = self.inner.state.lock() else {
                return;
            };
            let Some(stack) = s.profile.stack(index as usize).cloned() else {
                return;
            };
            let mut rebuild = false;
            for slot in &stack.slots {
                let Some(lane) = s.lanes.get_mut(&slot.layer) else {
                    continue;
                };
                lane.muted = slot.muted;
                lane.gain_db = slot.gain_db;
                // An empty scene patch keeps whatever the lane holds — the
                // scene rides levels, it doesn't force a reload.
                if !slot.patch.is_empty()
                    && lane.modules.first().is_some_and(|m| m.patch != slot.patch)
                {
                    if let Some(m) = lane.modules.first_mut() {
                        m.patch = slot.patch.clone();
                    }
                    lane.preset.clear();
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
            let Some(stack) = s.profile.stacks.get_mut(index as usize) else {
                return;
            };
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

// ── shared RigCore (mounted instance-scoped as "keys") ───────────────────────
impl signal_rigs_proto::rig_core::RigCore for KeysRigBackend {
    fn start(&self) {
        KeysRigSvc::start(self);
    }
    fn stop(&self) {
        KeysRigSvc::stop(self);
    }
    fn running(&self) -> bool {
        architect::rig::RigBackend::is_running(self)
    }
    fn presets(&self) -> Vec<signal_rigs_proto::RigPresetInfo> {
        KeysRigSvc::presets(self)
            .into_iter()
            .map(|p| signal_rigs_proto::RigPresetInfo {
                name: p.name,
                loaded: p.loaded,
            })
            .collect()
    }
    fn load_preset(&self, index: u32) {
        KeysRigSvc::load_preset(self, index);
    }
    fn midi_ports(&self) -> Vec<String> {
        KeysRigSvc::midi_ports(self)
    }
    fn set_midi_port(&self, name: String) {
        KeysRigSvc::set_midi_port(self, name);
    }
    fn midi_recent(&self) -> Vec<String> {
        KeysRigSvc::midi_recent(self)
            .iter()
            .map(|e| format!("{e:?}"))
            .collect()
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
    MidiEvent::NoteOn {
        channel: Channel::new(0),
        key: KeyNumber::new(note),
        velocity: Velocity::new(velocity),
    }
}

/// Fader travel — matches a console strip (−∞…+6 dB, clamped at −60).
const MIN_FADER_DB: f32 = -60.0;
const MAX_FADER_DB: f32 = 6.0;

/// Load the active profile: `FTS_KEYS_PROFILE` (a `.styx` file) if set and
/// parseable, else the built-in Worship profile.
fn load_profile() -> KeysProfile {
    let Ok(path) = std::env::var("FTS_KEYS_PROFILE") else {
        // The player's own copy, if they have edited their mixer. Its engines
        // are re-aligned to the built-in's, so a profile saved before an
        // engine existed still gets it — the saved file decides the ORDER of
        // what it knows, the built-in decides what there is.
        let built_in = worship_profile();
        return match KeysProfile::load_saved(&built_in.name) {
            Some(saved) => {
                let mut merged = built_in;
                merged.apply_order(&saved.engine_order());
                merged
            }
            None => built_in,
        };
    };
    match std::fs::read_to_string(&path)
        .map_err(|e| e.to_string())
        .and_then(|t| KeysProfile::from_styx_str(&t))
    {
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
            Container::layer("Layer A")
                .add(Container::module("Piano Source").sample_block("Piano", spec)),
        ),
    )
}

/// `scan_keyscape` with same-name non-pack presets removed.
fn prefer_packs_from_scan() -> (Vec<KeysPreset>, Vec<PathBuf>) {
    let (p, s) = scan_keyscape();
    prefer_packs(p, s)
}

/// Drop a preset whose name is already held by a `.signalpack`.
///
/// A pack and an Omnisphere patch can carry the same name — "Choir Men Ohs  ^"
/// is both a built pack and a factory `.prt_omn` — and only the pack is
/// loadable today: handing a patch file to the sampler's spec loader fails the
/// lane outright with `spec parse error: unexpected token`. Whichever was
/// scanned last used to win, which made it a coin toss.
///
/// Packs are kept, duplicates behind them are dropped. When the patch path
/// learns to realize through `omni_import::patch_to_container`, this becomes a
/// preference rather than an exclusion.
fn prefer_packs(presets: Vec<KeysPreset>, specs: Vec<PathBuf>) -> (Vec<KeysPreset>, Vec<PathBuf>) {
    let is_pack = |p: &PathBuf| {
        p.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("signalpack"))
    };
    let packed: std::collections::HashSet<String> = presets
        .iter()
        .zip(specs.iter())
        .filter(|(_, s)| is_pack(s))
        .map(|(p, _)| p.name.to_lowercase())
        .collect();
    let mut out_p = Vec::with_capacity(presets.len());
    let mut out_s = Vec::with_capacity(specs.len());
    let mut dropped = 0usize;
    for (p, s) in presets.into_iter().zip(specs) {
        if !is_pack(&s) && packed.contains(&p.name.to_lowercase()) {
            dropped += 1;
            continue;
        }
        out_p.push(p);
        out_s.push(s);
    }
    if dropped > 0 {
        tracing::info!(
            dropped,
            "keys rig: non-pack presets shadowed by a pack of the same name"
        );
    }
    (out_p, out_s)
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
    let (omni, omni_specs) = scan_packs_recursive_as(&omni_root, "Soundsource", "module", "Synth");
    packs.extend(omni);
    pack_specs.extend(omni_specs);
    // The NI Essential Pianos. A pack is a whole lane's worth of instrument
    // (the Piano packs) or its pedal-down resonance layer, which loads on its
    // own so a tight-memory rig can leave it out — see `ni-pianos.styx`.
    let ni_root =
        std::env::var("FTS_NI_PIANO_PACKS").unwrap_or_else(|_| NI_PIANO_PACKS_ROOT.into());
    let (ni, ni_specs) = scan_packs_recursive_as(&ni_root, "Grand", "layer", "Keys");
    tracing::info!(packs = ni.len(), "keys rig: NI piano packs");
    packs.extend(ni);
    pack_specs.extend(ni_specs);
    // Authored Omnisphere patches — these open into a whole layer.
    let patch_root =
        std::env::var("FTS_OMNISPHERE_PATCHES").unwrap_or_else(|_| OMNISPHERE_PATCH_ROOT.into());
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
        let mut dirs: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        for dir in dirs {
            let styx = dir.join("library.styx");
            if styx.exists() {
                let name = dir
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                if name.is_empty() {
                    continue;
                }
                let kind = kind_of(&name);
                let tags = tags_for(&kind, &name);
                let variants = crate::variations::variation_names(&name);
                presets.push(KeysPreset {
                    kind,
                    name,
                    loaded: false,
                    scope: "layer".into(),
                    tags,
                    variants,
                });
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
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut found: Vec<PathBuf> = Vec::new();
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("prt_omn"))
            {
                found.push(p);
            }
        }
        found.sort();
        for patch in found {
            let Some(name) = patch.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            // Factory names carry a library prefix ("KEY │ American Obesity").
            let display = name.rsplit('│').next().unwrap_or(name).trim().to_string();
            if display.is_empty() {
                continue;
            }
            // An Omnisphere patch is authored across a layer's modules.
            let tags = tags_for("Synth", &display);
            presets.push(KeysPreset {
                kind: "Patch".into(),
                name: display,
                loaded: false,
                scope: "layer".into(),
                tags,
                variants: crate::variations::variation_names(name),
            });
            specs.push(patch);
        }
    }
    (presets, specs)
}

/// Root of the built NI Essential Piano packs (`<Library>/<Library> - <Pack>`,
/// so this is scanned recursively). Override with `FTS_NI_PIANO_PACKS`.
///
/// The **Full** tree by default: these are the rig's primary pianos and the
/// proxy tier is audibly lossy (peak error ~3.7e-2 against source). Point this
/// at `Libraries/Proxy/Keys` on a machine that cannot spare the disk.
const NI_PIANO_PACKS_ROOT: &str = "/run/media/AudioHaven/Signal/Libraries/Full/Keys";

/// Root of the built Omnisphere soundsource packs — the synth half of the
/// shared library. Override with `FTS_OMNISPHERE_PACKS`.
const OMNISPHERE_PACKS_ROOT: &str = "/run/media/AudioHaven/Signal/Libraries/Keys/Omnisphere/Packs";

/// Enumerate `*.signalpack` files under `root`, at any depth (the Omnisphere
/// library nests by family; the NI pianos nest by library). The file stem is
/// the source name. `kind`/`scope`/`tag` say what the caller's tree holds — a
/// soundsource fills one module, a piano fills a whole lane.
fn scan_packs_recursive_as(
    root: &str,
    kind: &str,
    scope: &str,
    tag_kind: &str,
) -> (Vec<KeysPreset>, Vec<PathBuf>) {
    let mut presets = Vec::new();
    let mut specs = Vec::new();
    let mut stack = vec![PathBuf::from(root)];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut found: Vec<PathBuf> = Vec::new();
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("signalpack"))
            {
                found.push(p);
            }
        }
        found.sort();
        for pack in found {
            let Some(name) = pack.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if name.is_empty() {
                continue;
            }
            let tags = tags_for(tag_kind, name);
            presets.push(KeysPreset {
                kind: kind.to_string(),
                name: name.to_string(),
                loaded: false,
                scope: scope.to_string(),
                tags,
                variants: crate::variations::variation_names(name),
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
            .filter(|p| {
                p.extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("signalpack"))
            })
            .collect();
        packs.sort();
        for pack in packs {
            let name = pack
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            if name.is_empty() {
                continue;
            }
            let kind = kind_of(&name);
            let tags = tags_for(&kind, &name);
            let variants = crate::variations::variation_names(&name);
            presets.push(KeysPreset {
                kind,
                name,
                loaded: false,
                scope: "layer".into(),
                tags,
                variants,
            });
            specs.push(pack);
        }
    }
    (presets, specs)
}

/// Broad category for grouping in the browser.
/// Whether an engine is a drone — a pad player rather than something the
/// keyboard reaches. Named, because the profile has no flag for it yet.
fn is_drone(engine: &str) -> bool {
    engine.eq_ignore_ascii_case("drone")
}

fn kind_of(name: &str) -> String {
    let n = name.to_ascii_lowercase();
    if n.contains("grand") || (n.contains("piano") && !n.contains("e piano")) {
        "Grand".into()
    } else if n.contains("rhodes") {
        "Rhodes".into()
    } else if n.contains("wurl") {
        "Wurlitzer".into()
    } else if n.contains("clav") {
        "Clav".into()
    } else if n.contains("mks")
        || n.contains("mk-80")
        || n.contains("e piano")
        || n.contains("electric")
    {
        "Electric".into()
    } else if n.contains("toy")
        || n.contains("celeste")
        || n.contains("glock")
        || n.contains("bell")
    {
        "Toy/Bell".into()
    } else {
        "Other".into()
    }
}

/// Which engines a preset belongs to. A library instrument is Keys work; the
/// Omnisphere side is Synth, and the names that read as organ or pad get those
/// engines too, so a Pad lane's browser is pads rather than the whole library.
fn tags_for(kind: &str, name: &str) -> Vec<String> {
    let n = name.to_ascii_lowercase();
    let mut tags = Vec::new();
    if n.contains("organ") || n.contains("b3") || n.contains("farf") || n.contains("vox ") {
        tags.push("Organ".to_string());
    }
    if n.contains("drone") || n.contains("sustain") || n.contains("bed") {
        tags.push("Drone".to_string());
    }
    if n.contains("riser")
        || n.contains("impact")
        || n.contains("swell")
        || n.contains("noise")
        || n.contains("whoosh")
        || n.contains("hit")
    {
        tags.push("SFX".to_string());
    }
    if n.contains("pad") || n.contains("string") || n.contains("atmos") || n.contains("choir") {
        tags.push("Pad".to_string());
    }
    match kind {
        "Synth" | "Patch" | "Soundsource" => tags.push("Aux".to_string()),
        _ => tags.push("Keys".to_string()),
    }
    tags
}

fn slug(s: &str) -> String {
    s.to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// Convert a composition [`Container`] into a wire [`KeysNode`] tree.
fn node_of(c: &Container, parent: &str) -> KeysNode {
    let id = if parent.is_empty() {
        slug(&c.name)
    } else {
        format!("{parent}/{}", slug(&c.name))
    };
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
    KeysNode {
        id,
        label: c.name.clone(),
        role: role_tag(c.role),
        live: any_live,
        children,
    }
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

#[cfg(test)]
mod tests {
    //! The Global Control rule, at the level where it is new: an ENGINE knob
    //! over lanes that disagree. No audio, no library — the maths is pure
    //! state, so it is tested as state.

    use super::*;

    /// A worship-profile state whose named lanes each hold one sounding
    /// module at the given cutoff.
    fn keys_state(lanes: &[(&str, f32)]) -> State {
        let mut s = State::default();
        let mut profile = worship_profile();
        // These tests exercise the knob mechanics over the named lanes;
        // the worship profile's authored exclusions (Keys A, the piano) are
        // covered by their own test below.
        for (lane, _) in lanes {
            if let Some(l) = profile.layer_mut(lane) {
                l.exclude_global = false;
            }
        }
        s.adopt_profile(profile);
        for (lane, cutoff) in lanes {
            let l = s.lanes.get_mut(*lane).expect("lane in the worship profile");
            let mut macros = default_macros();
            macros.insert("filter.cutoff".into(), *cutoff);
            l.modules = vec![ModuleState {
                patch: "test".into(),
                macros,
                ..ModuleState::default()
            }];
        }
        s
    }

    /// The worship profile keeps the piano (Keys A) OUT of the global scope
    /// — the exclusion that regressed the knob tests when it was authored.
    #[test]
    fn excluded_lanes_stay_out_of_global_scope() {
        let mut s = State::default();
        s.adopt_profile(worship_profile());
        for lane in ["Keys 1", "Keys 2"] {
            let l = s.lanes.get_mut(lane).expect("lane");
            l.modules = vec![ModuleState {
                patch: "test".into(),
                macros: default_macros(),
                ..ModuleState::default()
            }];
        }
        let targets = keys_targets(&s);
        assert!(
            !targets.iter().any(|(lane, _)| lane == "Keys 1"),
            "the piano is excluded from the engine's global scope"
        );
        assert!(targets.iter().any(|(lane, _)| lane == "Keys 2"));
    }

    fn cutoff(s: &State, lane: &str) -> f32 {
        KeysRigBackend::module_value(s.lanes.get(lane).expect("lane"), 0, "filter.cutoff")
    }

    fn keys_targets(s: &State) -> Vec<(String, usize)> {
        KeysRigBackend::scope_targets(s, &KeysRigBackend::engine_lanes(s, "Keys"))
    }

    #[test]
    fn an_engine_knob_writes_through_while_its_lanes_agree() {
        let mut s = keys_state(&[("Keys 1", 8000.0), ("Keys 2", 8000.0)]);
        let targets = keys_targets(&s);
        let def = global_def("e.filter.cutoff").expect("engine cutoff");
        let span =
            KeysRigBackend::drive_global(&mut s, def, "filter.cutoff", &targets, None, 4000.0);

        assert!(
            span.is_none(),
            "an agreeing engine is absolute — nothing to remember"
        );
        assert_eq!(cutoff(&s, "Keys 1"), 4000.0);
        assert_eq!(cutoff(&s, "Keys 2"), 4000.0);

        let est = s.engines.get("Keys").cloned().expect("engine");
        let models = KeysRigBackend::global_models(&s, ENGINE, &targets, &est.globals, &est.spans);
        let read = models
            .iter()
            .find(|m| m.id == "e.filter.cutoff")
            .expect("model");
        assert!(!read.bipolar, "and it reads back as the value itself");
        assert_eq!(read.value, 4000.0);
    }

    #[test]
    fn an_engine_knob_offsets_lanes_that_disagree_and_gives_them_back() {
        let mut s = keys_state(&[("Keys 1", 2000.0), ("Keys 2", 8000.0)]);
        let targets = keys_targets(&s);
        let def = global_def("e.filter.cutoff").expect("engine cutoff");

        let span = KeysRigBackend::drive_global(&mut s, def, "filter.cutoff", &targets, None, 0.5)
            .expect("a disagreeing engine holds its baseline");
        let (a, b) = (cutoff(&s, "Keys 1"), cutoff(&s, "Keys 2"));
        assert!(a > 2000.0 && b > 8000.0, "both lanes travelled: {a} {b}");
        assert!(
            (b / a - 4.0).abs() < 1e-2,
            "and kept their spacing: {a} {b}"
        );
        assert_eq!(span.1.len(), 2, "the baseline spans both lanes");

        // Back to the detent: the patch's own values come back untouched.
        let back =
            KeysRigBackend::drive_global(&mut s, def, "filter.cutoff", &targets, Some(span), 0.0);
        assert!(back.is_none(), "centred — the span is dropped");
        assert!((cutoff(&s, "Keys 1") - 2000.0).abs() < 1e-2);
        assert!((cutoff(&s, "Keys 2") - 8000.0).abs() < 1e-2);
    }

    #[test]
    fn a_rig_knob_reaches_every_engine() {
        // Two engines, one lane each, disagreeing — the rig level should
        // still move both together and hand them back at the detent.
        let mut s = keys_state(&[("Keys 1", 2000.0), ("Pad", 8000.0)]);
        let targets = KeysRigBackend::scope_targets(&s, &KeysRigBackend::all_lanes(&s));
        let def = global_def("r.filter.cutoff").expect("rig cutoff");

        // Downward: the profile's other lanes sit wide open at 20 kHz, so
        // upward there is nowhere to go — the ceiling is the group's leader.
        let span = KeysRigBackend::drive_global(&mut s, def, "filter.cutoff", &targets, None, -0.5)
            .expect("a disagreeing rig holds its baseline");
        let (keys, pad) = (cutoff(&s, "Keys 1"), cutoff(&s, "Pad"));
        assert!(
            keys < 2000.0 && pad < 8000.0,
            "both engines travelled: {keys} {pad}"
        );
        assert!(
            (pad / keys - 4.0).abs() < 1e-2,
            "and kept their spacing: {keys} {pad}"
        );

        let back =
            KeysRigBackend::drive_global(&mut s, def, "filter.cutoff", &targets, Some(span), 0.0);
        assert!(back.is_none());
        assert!((cutoff(&s, "Keys 1") - 2000.0).abs() < 1e-2);
        assert!((cutoff(&s, "Pad") - 8000.0).abs() < 1e-2);
    }

    #[test]
    fn an_edit_underneath_rebases_the_engine_knob() {
        let mut s = keys_state(&[("Keys 1", 2000.0), ("Keys 2", 8000.0)]);
        s.engines.get_mut("Keys").expect("engine").spans.insert(
            "e.filter.cutoff".into(),
            (0.5, vec![("Keys 1".into(), 0, 2000.0)]),
        );

        KeysRigBackend::rebase_others(
            &mut s,
            &["Keys".to_string()],
            &["Keys 1".to_string()],
            "filter.cutoff",
            "",
        );

        assert!(
            s.engines.get("Keys").expect("engine").spans.is_empty(),
            "the engine's baseline described a patch that no longer exists"
        );
    }
}

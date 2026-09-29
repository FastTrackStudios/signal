//! Headless keys-rig backend — the vox-served core behind the detachable GUI.
//!
//! Owns a live [`KeysRig`] (composition-tree instrument on the shared engine),
//! scans the Keyscape library for presets, loads a single-instrument program
//! per preset, and plays it from hardware MIDI or UI notes. Implements the
//! [`signal_keys_proto::keys::KeysRig`] service + its `#[subscribe]` stream;
//! mount `router()` (`architect::rig::RigBackend`) on a vox transport.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::{Arc, Mutex};

use architect::dispatch::CurrentThreadDispatcher;
use architect::rig::RigBackend;
use architect::{HasDispatcher, Layer, PubSub, Services, layers};
use daw_audio_io::AudioIoPrefs;
use midicore::MidiEvent;
use signal_keys_proto::keys::{KeysEvent, KeysRig as KeysRigSvc, KeysRigStreamSource};
use signal_keys_proto::{
    KeysEngineDetail, KeysEngineModel, KeysLaneProgram, KeysLayerDetail, KeysLayerModel, KeysMacro,
    KeysMeter, KeysMixer, KeysNode, KeysPackRef, KeysPerform, KeysPreset, KeysStack, KeysStatus,
};

use crate::profile::{KeysProfile, worship_profile};
use signal_rig_host::mixer::{self as rig_mixer, db_to_linear};
use signal_sampler::Container;
use signal_sampler::rig_node::{RigNode, Role};

use crate::KeysRig;

// The local Keyscape extraction (per-instrument dirs each holding a
// `library.styx`) is `Keys/Keyscape` in the sampled tree — a fallback when no
// packs are present. Override with `FTS_KEYSCAPE_ROOT`.
/// The built `.signalpack` library — every pack root below is a folder in
/// it, so one setting moves them all: `FTS_PACK_LIBRARY` (the sampler's own
/// library setting — a drive mounted elsewhere, `/Volumes/…` on a Mac), else
/// the studio machine's mount.
fn pack_library() -> PathBuf {
    std::env::var("FTS_PACK_LIBRARY")
        .ok()
        .filter(|s| !s.is_empty())
        .map_or_else(
            || PathBuf::from("/run/media/AudioHaven/Signal/Libraries"),
            PathBuf::from,
        )
}

/// The raw sample and patch tree (the extractions the packs were built from,
/// and the instruments' own patch files): `FTS_SAMPLED_ROOT`, else the studio
/// machine's mount.
fn sampled_root() -> PathBuf {
    std::env::var("FTS_SAMPLED_ROOT")
        .ok()
        .filter(|s| !s.is_empty())
        .map_or_else(
            || PathBuf::from("/run/media/AudioHaven/Sampled"),
            PathBuf::from,
        )
}

/// A root in the sampled tree: its own override variable, else `rel` in it.
fn sampled_path(var: &str, rel: &str) -> String {
    std::env::var(var)
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| sampled_root().join(rel).to_string_lossy().into_owned())
}

/// A pack root: its own override variable, else `rel` in the pack library.
fn pack_root(var: &str, rel: &str) -> String {
    std::env::var(var)
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| pack_library().join(rel).to_string_lossy().into_owned())
}
#[derive(Default)]
struct State {
    presets: Vec<KeysPreset>,
    /// Absolute `library.styx` spec path per preset (index-aligned).
    specs: Vec<PathBuf>,
    loaded: Option<usize>,
    /// The loaded composition tree (for the control-view structure).
    tree: Option<Container>,
    midi_port: Option<String>,
    /// The rig's subscription to the shared process-wide MIDI hub. Dropping
    /// it detaches this rig only; the hardware ports stay open for the others.
    midi_handle: Option<signal_rig_host::midi_hub::Subscription>,
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
    /// The filters the mod wheel moves, by `(lane, module, leaf)`: the
    /// cutoff it moves each from (the patch's, normalized) and the last it
    /// wrote. A reading that is no longer the last write means the lane was
    /// rebuilt under it, and the reading is the patch's value again.
    wheel_filters: BTreeMap<(String, String, String), (f64, f64)>,
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
    /// The Omnisphere patch file this lane's module knobs were seeded from
    /// (`None`: not an Omnisphere lane, or not seeded yet). A lane hosting a
    /// whole `.prt_omn` gets one module per patch layer, filled from the
    /// patch, so its knobs start where the patch is — see `seed_omni_lanes`.
    omni_seed: Option<PathBuf>,
    /// Each module's knobs exactly as the patch seeded them (before the
    /// player's saved values) — the baseline an Omnisphere lane's knobs are
    /// applied relative to, so an untouched lane plays the imported patch
    /// as the importer built it.
    omni_base: Vec<BTreeMap<String, f32>>,
    /// FX bypass from this lane's scope or any above it (layer, engine,
    /// rig): the modules' effects are left out while it is on.
    fx_gated: bool,
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
    /// The patch `macros` belong to. When `patch` changes (a stack recall, a
    /// browser pick) the knobs are reset to that patch's own — its saved
    /// values over the defaults — before anything is built; see
    /// `prepare_lanes`.
    macros_patch: Option<String>,
    gain_db: f32,
    enabled: bool,
}

impl Default for ModuleState {
    fn default() -> Self {
        Self {
            patch: String::new(),
            variant: String::new(),
            macros: BTreeMap::new(),
            macros_patch: None,
            gain_db: 0.0,
            enabled: true,
        }
    }
}

/// The engine the mod wheel sweeps the filters of — the gig's pad "Cutoff"
/// knob (CC77), which moved both Omni Pads parts' global filter cutoff
/// together (Omnisphere's MIDI-learn `p0_gpfltc` / `p1_gpfltc`). At rest the
/// patches play as imported; all the way up opens them to 20 kHz, the group
/// keeping its spacing (see `KeysRigBackend::wheel_cutoff`).
const WHEEL_CUTOFF_ENGINE: &str = "Pad";

/// No mod-wheel value waiting for the worker.
const NO_WHEEL: u32 = u32::MAX;

/// Lanes that start muted — a starting position, not a limitation: an
/// ordinary lane mute, undone in the mixer.
///
/// Club Europa's pulsing lead (Synth 2) starts muted: a song reaches for it,
/// it is not a bed under everything. (A lane mute, not a module switch: an
/// imported Omnisphere lane has no module gain for the switch to act on.)
fn starts_muted(lane: &str) -> bool {
    lane == "Synth 2"
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
}

/// The canonical macro surface every Signal Engine layer exposes. Grouped
/// into the layer-zoom's panels; the order here is the render order.
///
/// Which ones reach sound is decided by what they drive
/// (`KeysRigBackend::macro_is_dsp`), not stored here.
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
    },
    MacroDef {
        id: "source.pan",
        name: "Pan",
        group: "Source",
        default: 0.0,
        min: -1.0,
        max: 1.0,
        unit: "",
    },
    MacroDef {
        id: "source.transpose",
        name: "Transpose",
        group: "Source",
        default: 0.0,
        min: -24.0,
        max: 24.0,
        unit: "st",
    },
    MacroDef {
        id: "source.fine",
        name: "Fine",
        group: "Source",
        default: 0.0,
        min: -100.0,
        max: 100.0,
        unit: "c",
    },
    MacroDef {
        id: "source.unison",
        name: "Unison",
        group: "Source",
        default: 1.0,
        min: 1.0,
        max: 8.0,
        unit: "v",
    },
    MacroDef {
        id: "source.detune",
        name: "Detune",
        group: "Source",
        default: 0.1,
        min: 0.0,
        max: 2.0,
        unit: "",
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
    },
    MacroDef {
        id: "filter.reso",
        name: "Resonance",
        group: "Filter",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        unit: "",
    },
    MacroDef {
        id: "filter.env_amt",
        name: "Env Amt",
        group: "Filter",
        default: 0.0,
        min: -1.0,
        max: 1.0,
        unit: "",
    },
    MacroDef {
        id: "filter.keytrack",
        name: "Key Trk",
        group: "Filter",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        unit: "",
    },
    MacroDef {
        id: "filter.drive",
        name: "Drive",
        group: "Filter",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        unit: "",
    },
    MacroDef {
        id: "filter.mix",
        name: "Mix",
        group: "Filter",
        default: 1.0,
        min: 0.0,
        max: 1.0,
        unit: "",
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
    },
    MacroDef {
        id: "env1.attack",
        name: "Attack",
        group: "Env 1",
        default: 0.0,
        min: 0.0,
        max: 5000.0,
        unit: "ms",
    },
    MacroDef {
        id: "env1.hold",
        name: "Hold",
        group: "Env 1",
        default: 0.0,
        min: 0.0,
        max: 2000.0,
        unit: "ms",
    },
    MacroDef {
        id: "env1.decay",
        name: "Decay",
        group: "Env 1",
        default: 0.0,
        min: 0.0,
        max: 5000.0,
        unit: "ms",
    },
    MacroDef {
        id: "env1.sustain",
        name: "Sustain",
        group: "Env 1",
        default: 1.0,
        min: 0.0,
        max: 1.0,
        unit: "",
    },
    MacroDef {
        id: "env1.release",
        name: "Release",
        group: "Env 1",
        default: 120.0,
        min: 0.0,
        max: 8000.0,
        unit: "ms",
    },
    MacroDef {
        id: "env2.delay",
        name: "Delay",
        group: "Env 2",
        default: 0.0,
        min: 0.0,
        max: 2000.0,
        unit: "ms",
    },
    MacroDef {
        id: "env2.attack",
        name: "Attack",
        group: "Env 2",
        default: 5.0,
        min: 0.0,
        max: 5000.0,
        unit: "ms",
    },
    MacroDef {
        id: "env2.hold",
        name: "Hold",
        group: "Env 2",
        default: 0.0,
        min: 0.0,
        max: 2000.0,
        unit: "ms",
    },
    MacroDef {
        id: "env2.decay",
        name: "Decay",
        group: "Env 2",
        default: 300.0,
        min: 0.0,
        max: 5000.0,
        unit: "ms",
    },
    MacroDef {
        id: "env2.sustain",
        name: "Sustain",
        group: "Env 2",
        default: 0.7,
        min: 0.0,
        max: 1.0,
        unit: "",
    },
    MacroDef {
        id: "env2.release",
        name: "Release",
        group: "Env 2",
        default: 200.0,
        min: 0.0,
        max: 8000.0,
        unit: "ms",
    },
    MacroDef {
        id: "env3.delay",
        name: "Delay",
        group: "Env 3",
        default: 0.0,
        min: 0.0,
        max: 2000.0,
        unit: "ms",
    },
    MacroDef {
        id: "env3.attack",
        name: "Attack",
        group: "Env 3",
        default: 3.0,
        min: 0.0,
        max: 5000.0,
        unit: "ms",
    },
    MacroDef {
        id: "env3.hold",
        name: "Hold",
        group: "Env 3",
        default: 0.0,
        min: 0.0,
        max: 2000.0,
        unit: "ms",
    },
    MacroDef {
        id: "env3.decay",
        name: "Decay",
        group: "Env 3",
        default: 250.0,
        min: 0.0,
        max: 5000.0,
        unit: "ms",
    },
    MacroDef {
        id: "env3.sustain",
        name: "Sustain",
        group: "Env 3",
        default: 0.8,
        min: 0.0,
        max: 1.0,
        unit: "",
    },
    MacroDef {
        id: "env3.release",
        name: "Release",
        group: "Env 3",
        default: 150.0,
        min: 0.0,
        max: 8000.0,
        unit: "ms",
    },
    MacroDef {
        id: "env3.depth",
        name: "Depth",
        group: "Env 3",
        default: 0.0,
        min: -1.0,
        max: 1.0,
        unit: "",
    },
    MacroDef {
        id: "env3.dest",
        name: "Dest",
        group: "Env 3",
        default: 0.0,
        min: 0.0,
        max: 6.0,
        unit: "dest",
    },
    MacroDef {
        id: "env4.delay",
        name: "Delay",
        group: "Env 4",
        default: 0.0,
        min: 0.0,
        max: 2000.0,
        unit: "ms",
    },
    MacroDef {
        id: "env4.attack",
        name: "Attack",
        group: "Env 4",
        default: 200.0,
        min: 0.0,
        max: 5000.0,
        unit: "ms",
    },
    MacroDef {
        id: "env4.hold",
        name: "Hold",
        group: "Env 4",
        default: 0.0,
        min: 0.0,
        max: 2000.0,
        unit: "ms",
    },
    MacroDef {
        id: "env4.decay",
        name: "Decay",
        group: "Env 4",
        default: 600.0,
        min: 0.0,
        max: 5000.0,
        unit: "ms",
    },
    MacroDef {
        id: "env4.sustain",
        name: "Sustain",
        group: "Env 4",
        default: 0.6,
        min: 0.0,
        max: 1.0,
        unit: "",
    },
    MacroDef {
        id: "env4.release",
        name: "Release",
        group: "Env 4",
        default: 500.0,
        min: 0.0,
        max: 8000.0,
        unit: "ms",
    },
    MacroDef {
        id: "env4.depth",
        name: "Depth",
        group: "Env 4",
        default: 0.0,
        min: -1.0,
        max: 1.0,
        unit: "",
    },
    MacroDef {
        id: "env4.dest",
        name: "Dest",
        group: "Env 4",
        default: 0.0,
        min: 0.0,
        max: 6.0,
        unit: "dest",
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
    },
    MacroDef {
        id: "lfo1.depth",
        name: "Depth",
        group: "LFO 1",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        unit: "",
    },
    MacroDef {
        id: "lfo1.shape",
        name: "Shape",
        group: "LFO 1",
        default: 0.0,
        min: 0.0,
        max: 4.0,
        unit: "wave",
    },
    MacroDef {
        id: "lfo1.fade",
        name: "Fade In",
        group: "LFO 1",
        default: 0.0,
        min: 0.0,
        max: 4000.0,
        unit: "ms",
    },
    MacroDef {
        id: "lfo1.dest",
        name: "Dest",
        group: "LFO 1",
        default: 0.0,
        min: 0.0,
        max: 6.0,
        unit: "dest",
    },
    MacroDef {
        id: "lfo2.rate",
        name: "Rate",
        group: "LFO 2",
        default: 0.5,
        min: 0.01,
        max: 40.0,
        unit: "Hz",
    },
    MacroDef {
        id: "lfo2.depth",
        name: "Depth",
        group: "LFO 2",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        unit: "",
    },
    MacroDef {
        id: "lfo2.shape",
        name: "Shape",
        group: "LFO 2",
        default: 1.0,
        min: 0.0,
        max: 4.0,
        unit: "wave",
    },
    MacroDef {
        id: "lfo2.fade",
        name: "Fade In",
        group: "LFO 2",
        default: 0.0,
        min: 0.0,
        max: 4000.0,
        unit: "ms",
    },
    MacroDef {
        id: "lfo2.dest",
        name: "Dest",
        group: "LFO 2",
        default: 0.0,
        min: 0.0,
        max: 6.0,
        unit: "dest",
    },
    MacroDef {
        id: "lfo3.rate",
        name: "Rate",
        group: "LFO 3",
        default: 4.0,
        min: 0.01,
        max: 40.0,
        unit: "Hz",
    },
    MacroDef {
        id: "lfo3.depth",
        name: "Depth",
        group: "LFO 3",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        unit: "",
    },
    MacroDef {
        id: "lfo3.shape",
        name: "Shape",
        group: "LFO 3",
        default: 2.0,
        min: 0.0,
        max: 4.0,
        unit: "wave",
    },
    MacroDef {
        id: "lfo3.fade",
        name: "Fade In",
        group: "LFO 3",
        default: 0.0,
        min: 0.0,
        max: 4000.0,
        unit: "ms",
    },
    MacroDef {
        id: "lfo3.dest",
        name: "Dest",
        group: "LFO 3",
        default: 0.0,
        min: 0.0,
        max: 6.0,
        unit: "dest",
    },
    MacroDef {
        id: "lfo4.rate",
        name: "Rate",
        group: "LFO 4",
        default: 8.0,
        min: 0.01,
        max: 40.0,
        unit: "Hz",
    },
    MacroDef {
        id: "lfo4.depth",
        name: "Depth",
        group: "LFO 4",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        unit: "",
    },
    MacroDef {
        id: "lfo4.shape",
        name: "Shape",
        group: "LFO 4",
        default: 3.0,
        min: 0.0,
        max: 4.0,
        unit: "wave",
    },
    MacroDef {
        id: "lfo4.fade",
        name: "Fade In",
        group: "LFO 4",
        default: 0.0,
        min: 0.0,
        max: 4000.0,
        unit: "ms",
    },
    MacroDef {
        id: "lfo4.dest",
        name: "Dest",
        group: "LFO 4",
        default: 0.0,
        min: 0.0,
        max: 6.0,
        unit: "dest",
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
    },
    MacroDef {
        id: "tone.drive",
        name: "Drive",
        group: "Tone",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        unit: "",
    },
    MacroDef {
        id: "tone.body",
        name: "Body",
        group: "Tone",
        default: 0.5,
        min: 0.0,
        max: 1.0,
        unit: "",
    },
    MacroDef {
        id: "vib.rate",
        name: "Rate",
        group: "Vibrato",
        default: 5.0,
        min: 0.1,
        max: 12.0,
        unit: "Hz",
    },
    MacroDef {
        id: "vib.depth",
        name: "Depth",
        group: "Vibrato",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        unit: "",
    },
    MacroDef {
        id: "vib.delay",
        name: "Delay",
        group: "Vibrato",
        default: 300.0,
        min: 0.0,
        max: 3000.0,
        unit: "ms",
    },
    MacroDef {
        id: "amb.bypass",
        name: "Bypass",
        group: "Ambience",
        // Off until asked for: every patch has its own space already.
        default: 1.0,
        min: 0.0,
        max: 1.0,
        unit: "bypass",
    },
    MacroDef {
        id: "amb.algo",
        name: "Algorithm",
        group: "Ambience",
        default: 1.0,
        min: 0.0,
        max: 14.0,
        unit: "",
    },
    MacroDef {
        id: "amb.size",
        name: "Size",
        group: "Ambience",
        default: 0.5,
        min: 0.0,
        max: 1.0,
        unit: "",
    },
    MacroDef {
        id: "amb.mix",
        name: "Mix",
        group: "Ambience",
        default: 0.15,
        min: 0.0,
        max: 1.0,
        unit: "",
    },
    MacroDef {
        id: "amb.predelay",
        name: "Pre-dly",
        group: "Ambience",
        default: 20.0,
        min: 0.0,
        max: 250.0,
        unit: "ms",
    },
    MacroDef {
        id: "amb.decay",
        name: "Decay",
        group: "Ambience",
        default: 0.45,
        min: 0.02,
        max: 1.0,
        unit: "",
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
        unit: "bypass",
    },
    MacroDef {
        id: "dly.algo",
        name: "Machine",
        group: "Delay",
        default: 0.0,
        min: 0.0,
        max: 12.0,
        unit: "",
    },
    MacroDef {
        id: "dly.div",
        name: "Div",
        group: "Delay",
        default: 3.0,
        min: 0.0,
        max: 7.0,
        unit: "div",
    },
    MacroDef {
        id: "dly.time",
        name: "Time",
        group: "Delay",
        default: 375.0,
        min: 20.0,
        max: 2000.0,
        unit: "ms",
    },
    MacroDef {
        id: "dly.feedback",
        name: "Feedback",
        group: "Delay",
        default: 0.35,
        min: 0.0,
        max: 0.95,
        unit: "",
    },
    MacroDef {
        id: "dly.mix",
        name: "Mix",
        group: "Delay",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        unit: "",
    },
    MacroDef {
        id: "fx.chorus",
        name: "Chorus",
        group: "Effects",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        unit: "",
    },
    MacroDef {
        id: "fx.delay",
        name: "Delay",
        group: "Effects",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        unit: "",
    },
    MacroDef {
        id: "fx.width",
        name: "Width",
        group: "Effects",
        default: 0.5,
        min: 0.0,
        max: 1.0,
        unit: "",
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
    },
    // ── Ambience ─────────────────────────────────────────────────────────
    GlobalDef {
        key: "amb.bypass",
        name: "Bypass",
        group: "Ambience",
        target: Some("amb.bypass"),
        default: 1.0,
        min: 0.0,
        max: 1.0,
        unit: "bypass",
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
        unit: "bypass",
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
    },
    GlobalDef {
        key: "dly.div",
        name: "Div",
        group: "Delay",
        target: Some("dly.div"),
        default: 3.0,
        min: 0.0,
        max: 7.0,
        unit: "div",
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

/// Set macro `id`, clamped to its range (unknown ids are ignored).
fn set_macro_clamped(macros: &mut BTreeMap<String, f32>, id: &str, v: f32) {
    if let Some(def) = macro_def(id) {
        macros.insert(id.to_string(), v.clamp(def.min, def.max));
    }
}

/// A module's knobs from an imported Omnisphere layer: filter, envelopes,
/// unison — the values the patch itself plays with.
fn seed_module_macros(
    macros: &mut BTreeMap<String, f32>,
    m: &signal_synth::engine::ImportedModule,
) {
    set_macro_clamped(macros, "filter.cutoff", m.cutoff_hz);
    set_macro_clamped(macros, "filter.reso", m.resonance);
    set_macro_clamped(macros, "filter.env_amt", m.filter_env_depth);
    set_macro_clamped(macros, "source.unison", m.unison as f32);
    set_macro_clamped(macros, "source.detune", m.detune);
    if let Some((a, d, sus, r)) = m.amp_env {
        set_macro_clamped(macros, "env1.attack", a);
        set_macro_clamped(macros, "env1.decay", d);
        set_macro_clamped(macros, "env1.sustain", sus);
        set_macro_clamped(macros, "env1.release", r);
    }
    if let Some((a, d, sus, r)) = m.filter_env {
        set_macro_clamped(macros, "env2.attack", a);
        set_macro_clamped(macros, "env2.decay", d);
        set_macro_clamped(macros, "env2.sustain", sus);
        set_macro_clamped(macros, "env2.release", r);
    }
}

/// Whether `path` is an Omnisphere patch file (a whole voice, not a sample
/// source).
fn is_omni_patch(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("prt_omn") || e.eq_ignore_ascii_case("mlt_omn"))
}

/// One more tap at `now`: the tempo of the last (up to) four taps, once
/// there are two; a gap over 2.5 s starts the count again.
fn tap_bpm(taps: &mut Vec<std::time::Instant>, now: std::time::Instant) -> Option<f32> {
    if taps
        .last()
        .is_some_and(|t| now.duration_since(*t).as_secs_f32() > 2.5)
    {
        taps.clear();
    }
    taps.push(now);
    let n = taps.len();
    if n > 4 {
        taps.drain(..n - 4);
    }
    (taps.len() >= 2).then(|| {
        let span = taps[taps.len() - 1].duration_since(taps[0]).as_secs_f32();
        60.0 * (taps.len() - 1) as f32 / span.max(1e-3)
    })
}

#[cfg(test)]
mod tap_tests {
    use super::tap_bpm;
    use std::time::{Duration, Instant};

    #[test]
    fn steady_taps_read_their_tempo_and_a_pause_starts_over() {
        let t0 = Instant::now();
        let mut taps = Vec::new();
        assert_eq!(tap_bpm(&mut taps, t0), None, "one tap is no tempo");
        let mut bpm = None;
        for i in 1..6 {
            bpm = tap_bpm(&mut taps, t0 + Duration::from_millis(500 * i));
        }
        assert!((bpm.unwrap() - 120.0).abs() < 0.5);
        // A long pause: the next tap starts a new count.
        assert_eq!(tap_bpm(&mut taps, t0 + Duration::from_secs(10)), None);
    }
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
                        muted: starts_muted(&layer.name),
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
                        omni_seed: None,
                        omni_base: Vec::new(),
                        fx_gated: false,
                    },
                );
            }
        }
        // The profile's saved mixer settings (Tone, Limiter, FX Bypass) at
        // every scope.
        for engine in &profile.engines {
            if let Some(e) = self.engines.get_mut(&engine.name) {
                for v in &engine.scope_values {
                    e.globals.insert(format!("{ENGINE}{}", v.id), v.value);
                }
            }
            for layer in &engine.layers {
                if let Some(l) = self.lanes.get_mut(&layer.name) {
                    for v in &layer.scope_values {
                        l.globals.insert(format!("{LAYER}{}", v.id), v.value);
                    }
                }
            }
        }
        for v in &profile.scope_values {
            self.rig_globals.insert(format!("{RIG}{}", v.id), v.value);
        }
        // The rig's output limiter is a safety net, on unless a profile
        // turns it off: five hard notes stacked in one lane clip otherwise.
        self.rig_globals.entry(format!("{RIG}limiter")).or_insert(1.0);
        self.profile = profile;
        KeysRigBackend::refresh_fx_gates(self);
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

    /// Can anything this lane renders reach the output right now?
    ///
    /// Mute, solo-exclusion and the engine's own mute — the same three the
    /// daw track ops fold in, asked here so the render tree can skip work the
    /// track would only throw away.
    fn lane_is_audible(&self, lane: &LaneState) -> bool {
        if lane.muted {
            return false;
        }
        if self.engines.get(&lane.engine).is_some_and(|e| e.muted) {
            return false;
        }
        // Solo silences every un-soloed lane.
        !self.any_solo() || lane.soloed
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
    /// Debounce generation for saving the profile after knob edits.
    save_gen: std::sync::atomic::AtomicU64,
    /// Recent tempo taps.
    taps: Mutex<Vec<std::time::Instant>>,
    /// The mod wheel's latest value ([`NO_WHEEL`] when none is pending),
    /// handed from the MIDI thread to the `keys-wheel` worker.
    wheel: AtomicU32,
    /// That worker, to wake.
    wheel_worker: std::sync::OnceLock<std::thread::Thread>,
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

/// The output-artefact half of [`signal_keys_proto::KeysRealtime`], read from
/// the sampler's process-wide counters (`engine::output_glitches`).
///
/// Split out so `status` can spread it into the struct. These are global to
/// the process rather than per-rig, because the audio thread bumps them from
/// wherever it happens to be rendering.
fn artefacts() -> signal_keys_proto::KeysRealtime {
    let g = signal_sampler::engine::output_glitches();
    signal_keys_proto::KeysRealtime {
        holes: g.gap_runs as u64,
        hole_frames: g.gap_frames as u64,
        longest_hole: g.longest_gap as u64,
        clicks: g.click_frames as u64,
        nonfinite: g.nonfinite_frames as u64,
        ..Default::default()
    }
}

impl KeysRigBackend {
    /// The profile's full program exactly as the rig would build it now —
    /// lanes seeded, knobs applied — without opening audio. For offline
    /// probes (`examples/lane_render`).
    #[doc(hidden)]
    pub fn debug_profile_program(&self) -> Option<Container> {
        self.prepare_lanes();
        self.profile_program()
    }

    /// The profile as the per-lane daw-track program the app plays —
    /// lanes seeded, knobs applied — without opening audio
    /// (`examples/lane_stress`).
    #[doc(hidden)]
    pub fn debug_profile_lane_program(&self) -> Option<signal_sampler::keys_rig::LaneProgram> {
        self.prepare_lanes();
        self.profile_lane_program()
    }

    /// Host the profile headless (no audio device) as this backend's own
    /// rig, at the app's starting levels — so the live paths (knobs, the mod
    /// wheel) reach it exactly as they do in the app (`examples/rig_render`).
    #[doc(hidden)]
    pub fn debug_open_headless(&self, sample_rate: u32) -> bool {
        let Some(program) = self.debug_profile_lane_program() else {
            return false;
        };
        let Ok(rig) = KeysRig::open_headless(sample_rate, &program) else {
            return false;
        };
        if let Ok(mut slot) = self.inner.rig.lock() {
            *slot = Some(rig);
        }
        self.apply_mixer();
        true
    }

    /// Rebuild the hosted program now, as a knob that cannot go live does.
    #[doc(hidden)]
    pub fn debug_rebuild(&self) {
        self.rebuild_program();
    }

    /// Run `f` on the hosted rig.
    #[doc(hidden)]
    pub fn debug_with_rig<R>(&self, f: impl FnOnce(&KeysRig) -> R) -> Option<R> {
        self.inner.rig.lock().ok()?.as_ref().map(f)
    }

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
            .is_some_and(|l| l.exclude_global)
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
                        // A scope's own parameter (Tone, Limiter, FX
                        // bypass): its track's stage plays it.
                        live: true,
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
                        // Lit when it reaches sound: a DSP target, live.
                        live: Self::macro_is_dsp(target) && !targets.is_empty(),
                        bipolar: false,
                        spread: fmt_value(value, def.unit),
                    }
                } else {
                    let offset = spans.get(&id).map_or(0.0, |(o, _)| *o);
                    KeysMacro {
                        id,
                        name: def.name.to_string(),
                        group: def.group.to_string(),
                        value: offset,
                        min: -1.0,
                        max: 1.0,
                        unit: String::new(),
                        live: Self::macro_is_dsp(target),
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
        let (tmin, tmax) = tdef.map_or((def.min, def.max), |d| (d.min, d.max));
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
        let ratio = matches!(tdef.map(|d| d.unit), Some("Hz" | "ms"));
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
    /// Recompute every lane's FX gate from the FX Bypass switches of its
    /// own, its engine's and the rig's scope. Returns whether any changed.
    fn refresh_fx_gates(s: &mut State) -> bool {
        let on = |map: &BTreeMap<String, f32>, prefix: &str| {
            map.get(&format!("{prefix}fx.bypass"))
                .is_some_and(|v| *v >= 0.5)
        };
        let rig = on(&s.rig_globals, RIG);
        let engines: BTreeMap<String, bool> = s
            .engines
            .iter()
            .map(|(n, e)| (n.clone(), on(&e.globals, ENGINE)))
            .collect();
        let mut changed = false;
        for lane in s.lanes.values_mut() {
            let gated = rig
                || engines.get(&lane.engine).copied().unwrap_or(false)
                || on(&lane.globals, LAYER);
            changed |= lane.fx_gated != gated;
            lane.fx_gated = gated;
        }
        changed
    }

    /// Knobs that drive an imported patch's own modulators — its LFOs and
    /// Mod Env. They reach sound on a lane hosting an Omnisphere patch (its
    /// routes are the destinations); a keys module has none.
    fn macro_is_mod(id: &str) -> bool {
        ["lfo1.", "lfo2.", "lfo3.", "lfo4.", "env3.", "env4."]
            .iter()
            .any(|p| id.starts_with(p))
    }

    /// The modulator knobs an imported patch honours: its own LFOs' rate,
    /// depth, shape and fade, and its Mod Env's times. Destinations, the
    /// Mod Env depth and the second Mod Env are a keys module's — an imported
    /// patch routes its modulators itself.
    fn mod_reaches_import(id: &str) -> bool {
        let lfo = ["lfo1.", "lfo2.", "lfo3.", "lfo4."]
            .iter()
            .any(|p| id.starts_with(p))
            && !id.ends_with(".dest");
        let env = id.starts_with("env3.") && !matches!(id, "env3.depth" | "env3.dest");
        lfo || env
    }

    /// Knobs whose change builds a new effect engine (a reverb's algorithm
    /// and decay) — a coalesced rebuild, not a live write. A delay's machine
    /// switches live.
    fn macro_rebuilds(id: &str) -> bool {
        matches!(id, "amb.algo" | "amb.decay")
    }

    fn macro_is_dsp(id: &str) -> bool {
        id.starts_with("filter.")
            || id.starts_with("env1.")
            || id.starts_with("env2.")
            || id.starts_with("vib.")
            || id.starts_with("tone.")
            || id.starts_with("amb.")
            || id.starts_with("dly.")
            || matches!(id, "fx.chorus" | "fx.delay" | "fx.width")
            || matches!(
                id,
                "source.unison"
                    | "source.detune"
                    | "source.pan"
                    | "source.transpose"
                    | "source.fine"
            )
    }

    /// Push one module's knobs into the RUNNING engine, live: the filter
    /// and the Amp's pan / width / tone through the parameter overlay, the
    /// envelopes into their sources, the sampler's voice settings through its
    /// setters, the effects' values in their own units. Returns `false` when
    /// that is not enough — nothing hosted, or the chain itself must change
    /// (an effect switched on or off, a reverb algorithm or decay, which
    /// build a new engine) — and the caller rebuilds.
    fn push_module_dsp(&self, layer: &str, module_idx: usize) -> bool {
        use signal_sampler::native::AdsrParams;
        let set = {
            let Ok(s) = self.inner.state.lock() else {
                return false;
            };
            if !s.lanes.contains_key(layer) {
                return false;
            }
            Self::module_settings_for(&s.lanes, layer, module_idx)
        };
        // A keys module is "<lane> A"; an imported Omnisphere patch keeps
        // its own "Layer A".."Layer D". Both name their parts alike
        // ("Soundsource", "Filter 1", "Amp", "Amp Env", "Filter Env").
        let slot = signal_synth::engine::module_slot(module_idx);
        let modules = [format!("{layer} {slot}"), format!("Layer {slot}")];
        let Ok(rig) = self.inner.rig.lock() else {
            return false;
        };
        let Some(rig) = rig.as_ref() else {
            return false;
        };
        let sample_rate = rig.sample_rate().max(1);
        rig.edit_lane(layer, |inst| {
            let render = inst.render_mut();
            let mut applied = false;
            let mut rebuild = false;
            let adsr = |(a, d, sus, r): (f32, f32, f32, f32)| AdsrParams {
                attack_s: (a / 1000.0).max(0.0),
                decay_s: (d / 1000.0).max(0.0),
                sustain: sus.clamp(0.0, 1.0),
                release_s: (r / 1000.0).max(0.0),
            };
            let cutoff_norm = f64::from(signal_sampler::native::NativeFilter::norm_from_cutoff(
                set.cutoff_hz,
            ));
            for module in &modules {
                if !render.has_leaf(module, "Soundsource") {
                    continue;
                }
                // Envelope sources + the env→cutoff amount (module-level:
                // a synth module's, an imported patch's route).
                render.set_env(module, "Amp Env", adsr(set.amp_env));
                render.set_env(module, "Filter Env", adsr(set.filter_env));
                render.set_env(module, "Mod Env", adsr(set.mod_env));
                render.set_env(module, "Mod Env 2", adsr(set.mod_env2));
                for (i, name) in ["Mod Env", "Mod Env 2"].into_iter().enumerate() {
                    let (d, h) = set.mod_env_dh[i];
                    render.set_env_timing(module, name, d.max(0.0) / 1000.0, h.max(0.0) / 1000.0);
                }
                // An imported patch's LFOs are part-level (addressed by the
                // lane, their routes the patch's); a keys module's are its
                // own, routed to the destinations picked.
                let imported = module.starts_with("Layer ");
                let lfo_scope = if imported { layer } else { module.as_str() };
                for (n, &(rate, depth, wave)) in set.lfos.iter().enumerate() {
                    let name = format!("LFO {}", n + 1);
                    let wave = match wave.round() as u32 {
                        1 => signal_sampler::native::LfoWave::Triangle,
                        2 => signal_sampler::native::LfoWave::Saw,
                        3 => signal_sampler::native::LfoWave::Square,
                        4 => signal_sampler::native::LfoWave::SampleHold,
                        _ => signal_sampler::native::LfoWave::Sine,
                    };
                    let fade = Some(set.lfo_fade_ms[n].max(0.0) / 1000.0);
                    render.set_lfo(lfo_scope, &name, rate, Some(wave), fade);
                    if imported {
                        render.set_source_depth(layer, &name, depth);
                    }
                }
                if !imported {
                    // The routes the picked destinations need, against the
                    // routes the tree has: a different set is a rebuild; the
                    // same set takes its depths live.
                    let want = set.mod_routes();
                    for source in ["LFO 1", "LFO 2", "LFO 3", "LFO 4", "Mod Env", "Mod Env 2"] {
                        let mut have = render.routes_from(module, source);
                        have.sort();
                        let mut need: Vec<(String, String)> = want
                            .iter()
                            .filter(|r| r.source == source)
                            .map(|r| (r.leaf.to_lowercase(), r.param.to_lowercase()))
                            .collect();
                        need.sort();
                        if have != need {
                            rebuild = true;
                        }
                    }
                    for r in &want {
                        render.set_route_depth(module, &r.source, r.leaf, r.param, r.depth);
                    }
                }
                let routed = render.set_route_depth(
                    module,
                    "Filter Env",
                    "Filter 1",
                    "cutoff",
                    set.filter_env_depth.clamp(-1.0, 1.0),
                );
                // Sampler source: the per-voice ADSR, filter, vibrato,
                // tuning and unison.
                let mut voice_filter = false;
                let sampler = render.with_sampler_source(module, "Soundsource", |sampler| {
                    let engine = sampler.engine_mut();
                    let frames = |ms: f32| ((ms.max(0.0) / 1000.0) * sample_rate as f32) as usize;
                    engine.set_attack_frames(frames(set.amp_env.0));
                    engine.set_decay_frames(frames(set.amp_env.1));
                    engine.set_sustain_level(set.amp_env.2);
                    engine.set_release_frames(frames(set.amp_env.3));
                    // A patch that routes its own filter envelope keeps it;
                    // otherwise the voices' own filter carries the knobs.
                    let (amount, keytrack) = if routed {
                        (0.0, 0.0)
                    } else {
                        (set.filter_env_depth, set.keytrack)
                    };
                    engine.set_voice_filter(
                        adsr(set.filter_env),
                        amount,
                        set.cutoff_hz,
                        set.resonance,
                        keytrack,
                    );
                    voice_filter = engine.voice_filter_on();
                    engine.set_vibrato(set.vib_rate, set.vib_depth, set.vib_delay_ms);
                    engine.set_tune(set.transpose, set.fine);
                    engine.set_unison(
                        (set.unison.max(1)).min(8) as u8,
                        set.detune.clamp(0.0, 2.0) * 100.0,
                        0.7,
                    );
                });
                applied |= sampler;
                // The module's shared filter: opened while the voices filter.
                let (c, r) = if voice_filter {
                    (1.0, 0.0)
                } else {
                    (cutoff_norm, f64::from(set.resonance.clamp(0.0, 1.0)))
                };
                applied |= render.set_leaf_param(module, "Filter 1", "cutoff", c);
                render.set_leaf_param(module, "Filter 1", "resonance", r);
                render.set_leaf_param(
                    module,
                    "Filter 1",
                    "drive",
                    f64::from(set.filter_drive.clamp(0.0, 1.0)),
                );
                render.set_leaf_param(
                    module,
                    "Filter 1",
                    "mix",
                    f64::from(set.filter_mix.clamp(0.0, 1.0)),
                );
                // Per-voice synth sources (the oscillator / the wavetable):
                // ADSRs over an 8 s range, vibrato, tuning, their own filter.
                let seg = |ms: f32| f64::from(((ms.max(0.0) / 1000.0) / 8.0).clamp(0.0, 1.0));
                let ss = |r: &mut signal_sampler::node_render::RenderNode, p: &str, v: f64| {
                    r.set_leaf_param(module, "Soundsource", p, v)
                };
                applied |= ss(render, "amp_attack", seg(set.amp_env.0));
                ss(render, "amp_decay", seg(set.amp_env.1));
                ss(
                    render,
                    "amp_sustain",
                    f64::from(set.amp_env.2.clamp(0.0, 1.0)),
                );
                ss(render, "amp_release", seg(set.amp_env.3));
                ss(render, "filter_attack", seg(set.filter_env.0));
                ss(render, "filter_decay", seg(set.filter_env.1));
                ss(
                    render,
                    "filter_sustain",
                    f64::from(set.filter_env.2.clamp(0.0, 1.0)),
                );
                ss(render, "filter_release", seg(set.filter_env.3));
                ss(render, "cutoff", cutoff_norm);
                ss(
                    render,
                    "resonance",
                    f64::from(set.resonance.clamp(0.0, 1.0)),
                );
                ss(
                    render,
                    "env_amt",
                    f64::from(f32::midpoint(set.filter_env_depth.clamp(-1.0, 1.0), 1.0)),
                );
                ss(
                    render,
                    "vib_rate",
                    f64::from(set.vib_rate.clamp(0.0, 12.0) / 12.0),
                );
                ss(
                    render,
                    "vib_depth",
                    f64::from(set.vib_depth.clamp(0.0, 1.0)),
                );
                ss(render, "tune", f64::from(set.wavetable_tune()));
                // The Amp: pan, width, tone (normalized).
                let amp = set.amp_block();
                for p in amp.params.iter().filter(|p| p.name != "gain") {
                    if let Ok(v) = p.value.parse::<f64>() {
                        render.set_leaf_param(module, "Amp", &p.name, v);
                    }
                }
                // Effects: an effect switched on or off changes the chain —
                // a rebuild; one in place takes its values live, in its own
                // units. A reverb's algorithm and decay build a new engine,
                // so a change to either rebuilds too.
                let want: Vec<signal_sampler::rig::RigBlock> = set.fx_blocks();
                for name in ["Chorus", "Delay", "Ambience"] {
                    let wanted = want.iter().find(|b| b.display_name() == name);
                    let present = render.has_leaf(module, name);
                    match (wanted, present) {
                        (Some(b), true) => {
                            for p in &b.params {
                                let Ok(v) = p.value.parse::<f64>() else {
                                    continue;
                                };
                                // Algorithm and decay build a new engine:
                                // those knobs rebuild (`macro_rebuilds`).
                                if name == "Ambience"
                                    && matches!(p.name.as_str(), "algorithm" | "decay")
                                {
                                    continue;
                                }
                                render.set_leaf_plain(module, name, &p.name, v);
                            }
                        }
                        (None, false) => {}
                        _ => rebuild = true,
                    }
                }
            }
            applied && !rebuild
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
        let r#gen = self.inner.rebuild_gen.fetch_add(1, Ordering::AcqRel) + 1;
        let b = self.clone();
        let _ = std::thread::Builder::new()
            .name("keys-param-rebuild".into())
            .spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(180));
                if b.inner.rebuild_gen.load(Ordering::Acquire) != r#gen {
                    return; // a later edit owns the rebuild
                }
                let _rt = keys_runtime().enter();
                b.rebuild_program();
            });
    }

    /// A scope's own parameter moved (Tone, Limiter, FX Bypass): Tone and
    /// Limiter go live onto the scope stages; an FX Bypass that changed a
    /// lane's gate changes chains, so it rebuilds.
    fn after_scope_param(&self, regate: bool) {
        self.apply_mixer();
        self.remember_macros_soon();
        if regate {
            self.rebuild_soon();
        }
        self.publish_mixer();
    }

    fn after_global(&self, rebuild: bool, targets: &[(String, usize)]) {
        self.remember_macros_soon();
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
                .map_or("<first>", |p| p.name.as_str()),
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
            for m in &mut lane.modules {
                if m.patch.is_empty() {
                    continue;
                }
                if let Some(name) = canonical(&m.patch) {
                    m.patch = name
                } else {
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
                rebuild_gen: std::sync::atomic::AtomicU64::new(0),
                save_gen: std::sync::atomic::AtomicU64::new(0),
                taps: Mutex::new(Vec::new()),
                wheel: AtomicU32::new(NO_WHEEL),
                wheel_worker: std::sync::OnceLock::new(),
            }),
        };
        backend.spawn_meter_pump("keys-meter-pump");
        backend.spawn_tempo_watch();
        backend.spawn_wheel_worker();
        backend
    }

    /// Follow the band's tempo (`signal_rig_host::tempo`, the guitar rig's
    /// tap): when it changes, every module with a synced delay takes the new
    /// time, live.
    /// Hand a mod-wheel value to the `keys-wheel` worker (the latest wins).
    fn queue_wheel(inner: &Inner, value: u8) {
        inner
            .wheel
            .store(u32::from(value.min(127)), std::sync::atomic::Ordering::Relaxed);
        if let Some(worker) = inner.wheel_worker.get() {
            worker.unpark();
        }
    }

    /// Apply mod-wheel moves off the MIDI thread, coalesced: a sweep sends
    /// far more values than the cutoff needs to follow.
    fn spawn_wheel_worker(&self) {
        let weak = Arc::downgrade(&self.inner);
        let worker = std::thread::Builder::new()
            .name("keys-wheel".into())
            .spawn(move || loop {
                std::thread::park_timeout(std::time::Duration::from_millis(500));
                let Some(inner) = weak.upgrade() else { return };
                let v = inner.wheel.swap(NO_WHEEL, std::sync::atomic::Ordering::Relaxed);
                if v != NO_WHEEL {
                    Self { inner }.wheel_cutoff(v as u8);
                }
            });
        if let Ok(handle) = worker {
            let _ = self.inner.wheel_worker.set(handle.thread().clone());
        }
    }

    /// The mod wheel as the gig's pad Cutoff knob, played live like any
    /// modulation — never an edit, never a rebuild. At rest (0) every filter
    /// of [`WHEEL_CUTOFF_ENGINE`]'s lanes sits where its patch put it; up,
    /// all of them rise by the same number of octaves (the group keeps its
    /// spacing), the brightest reaching 20 kHz at 127. Moves each layer's
    /// Omnisphere filters (the cutoff overlay, which the filter envelope's
    /// routes ride on) and its sampler's per-voice filter.
    fn wheel_cutoff(&self, value: u8) {
        use signal_sampler::native::NativeFilter;
        const LEAVES: [&str; 2] = ["Filter 1", "Filter 2"];
        type Key = (String, String, String);
        let w = f32::from(value.min(127)) / 127.0;
        let (lanes, mut known) = {
            let Ok(mut s) = self.inner.state.lock() else {
                return;
            };
            if !s.engines.contains_key(WHEEL_CUTOFF_ENGINE) {
                return;
            }
            (
                Self::engine_lanes(&s, WHEEL_CUTOFF_ENGINE),
                std::mem::take(&mut s.wheel_filters),
            )
        };
        let modules = |lane: &str| -> Vec<String> {
            (0..4)
                .flat_map(|i| {
                    let slot = signal_synth::engine::module_slot(i);
                    [format!("{lane} {slot}"), format!("Layer {slot}")]
                })
                .collect()
        };
        let rig = self.inner.rig.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(rig) = rig.as_ref() else {
            return;
        };
        // Where every filter sits now — the patch's value, unless the wheel
        // put it there.
        let mut bases: Vec<(Key, f64)> = Vec::new();
        for lane in &lanes {
            rig.edit_lane(lane, |inst| {
                let render = inst.render_mut();
                for module in modules(lane) {
                    if !render.has_leaf(&module, "Soundsource") {
                        continue;
                    }
                    for leaf in LEAVES {
                        let Some(now) = render.leaf_param_value(&module, leaf, "cutoff") else {
                            continue;
                        };
                        let key = (lane.clone(), module.clone(), leaf.to_string());
                        let base = match known.get(&key) {
                            Some(&(base, wrote)) if (now - wrote).abs() < 1e-6 => base,
                            _ => now,
                        };
                        bases.push((key, base));
                    }
                }
            });
        }
        // The group's reach: the brightest filter still closed opens fully.
        let leader = bases
            .iter()
            .map(|(_, b)| *b)
            .filter(|b| *b < 0.999)
            .fold(None::<f64>, |m, b| Some(m.map_or(b, |m: f64| m.max(b))));
        let span = leader.map_or(0.0, |b| {
            (20_000.0 / NativeFilter::cutoff_from_norm(b as f32)).log2().max(0.0)
        });
        let octaves = w * span;
        known.clear();
        for lane in &lanes {
            rig.edit_lane(lane, |inst| {
                let render = inst.render_mut();
                for (key, base) in bases.iter().filter(|(k, _)| k.0 == *lane) {
                    let hz = NativeFilter::cutoff_from_norm(*base as f32) * octaves.exp2();
                    let v = if *base >= 0.999 {
                        *base
                    } else {
                        f64::from(NativeFilter::norm_from_cutoff(hz.min(20_000.0)))
                    };
                    render.set_leaf_param(&key.1, &key.2, "cutoff", v);
                    known.insert(key.clone(), (*base, v));
                }
                for module in modules(lane) {
                    render.with_sampler_source(&module, "Soundsource", |sampler| {
                        sampler.engine_mut().set_cutoff_mod(octaves);
                    });
                }
            });
        }
        drop(rig);
        if let Ok(mut s) = self.inner.state.lock() {
            s.wheel_filters = known;
        }
    }

    fn spawn_tempo_watch(&self) {
        let weak = Arc::downgrade(&self.inner);
        let _ = std::thread::Builder::new()
            .name("keys-tempo-watch".into())
            .spawn(move || {
                let mut seen = signal_rig_host::tempo::generation();
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(200));
                    let Some(inner) = weak.upgrade() else { return };
                    let now = signal_rig_host::tempo::generation();
                    if now == seen {
                        continue;
                    }
                    seen = now;
                    let b = Self { inner };
                    b.publish_perform();
                    let targets: Vec<(String, usize)> = b
                        .inner
                        .state
                        .lock()
                        .map(|s| {
                            s.lanes
                                .iter()
                                .flat_map(|(name, lane)| {
                                    (0..lane.modules.len()).filter_map(move |i| {
                                        let synced = lane.modules[i]
                                            .macros
                                            .get("dly.div")
                                            .is_some_and(|d| *d >= 0.5);
                                        synced.then(|| (name.clone(), i))
                                    })
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    for (lane, i) in targets {
                        b.push_module_dsp(&lane, i);
                    }
                }
            });
    }

    fn program_for(&self, index: usize) -> Option<Container> {
        let s = self.inner.state.lock().ok()?;
        let spec = s.specs.get(index)?.to_string_lossy().into_owned();
        let name = s.presets.get(index)?.name.clone();
        Some(keys_program(&name, spec))
    }

    /// The live profile + patch-spec index + lane snapshot behind both
    /// program builders (single tree and lane program).
    /// Seed every lane that hosts a whole Omnisphere patch it has not been
    /// seeded from: one module per patch layer (A..D), each module's knobs
    /// filled from that layer. Runs before every program build, so a lane's
    /// knobs describe the patch before anything is built from them; a lane
    /// whose patch changes is seeded again, from the new one.
    fn seed_omni_lanes(&self) {
        let todo: Vec<(String, PathBuf, String)> = {
            let Ok(s) = self.inner.state.lock() else {
                return;
            };
            s.lanes
                .iter()
                .filter_map(|(name, lane)| {
                    let patch = lane.modules.first()?.patch.clone();
                    let i = s.presets.iter().position(|p| p.name == patch)?;
                    let path = s.specs.get(i)?.clone();
                    (is_omni_patch(&path) && lane.omni_seed.as_ref() != Some(&path))
                        .then(|| (name.clone(), path, patch))
                })
                .collect()
        };
        for (name, path, patch) in todo {
            let layers = match signal_synth::engine::import_omni_layers(&path) {
                Ok(l) => l,
                Err(e) => {
                    tracing::warn!(lane = %name, ?path, "keys rig: Omnisphere seed failed: {e}");
                    continue;
                }
            };
            let lfos = signal_synth::engine::import_omni_lfos(&path).unwrap_or_default();
            let Ok(mut s) = self.inner.state.lock() else {
                return;
            };
            let Some(lane) = s.lanes.get_mut(&name) else {
                continue;
            };
            let enabled = lane.modules.first().is_none_or(|m| m.enabled);
            while lane.modules.len() < layers.len() {
                // Every module names the patch: the lane plays the whole
                // patch, and a module with no patch is skipped by the
                // Global Controls.
                lane.modules.push(ModuleState {
                    patch: patch.clone(),
                    macros: default_macros(),
                    enabled,
                    ..ModuleState::default()
                });
            }
            for (slot, m) in lane.modules.iter_mut().zip(&layers) {
                seed_module_macros(&mut slot.macros, m);
                // The part's LFOs, on every module (they are shared).
                for (n, (rate, depth, shape)) in lfos.iter().enumerate().take(4) {
                    let id = format!("lfo{}", n + 1);
                    set_macro_clamped(&mut slot.macros, &format!("{id}.rate"), *rate);
                    set_macro_clamped(&mut slot.macros, &format!("{id}.depth"), *depth);
                    set_macro_clamped(&mut slot.macros, &format!("{id}.shape"), *shape);
                }
            }
            lane.spans.clear();
            lane.omni_seed = Some(path);
            lane.omni_base = lane.modules.iter().map(|m| m.macros.clone()).collect();
            // The player's saved knobs for this patch go over the patch's own.
            let State { lanes, profile, .. } = &mut *s;
            if let Some(lane) = lanes.get_mut(&name) {
                let def = profile
                    .engines
                    .iter()
                    .flat_map(|e| &e.layers)
                    .find(|l| l.name == name);
                for (i, slot) in lane.modules.iter_mut().enumerate() {
                    if let Some(saved) = def.and_then(|d| d.saved_macros(&slot.patch, i as u32)) {
                        for v in saved {
                            set_macro_clamped(&mut slot.macros, &v.id, v.value);
                        }
                    }
                    slot.macros_patch = Some(slot.patch.clone());
                }
            }
            tracing::info!(lane = %name, layers = layers.len(), "keys rig: seeded Omnisphere lane knobs");
        }
    }

    /// Bring every module's knobs in line with the patch it now holds: an
    /// Omnisphere lane is seeded from its patch (`seed_omni_lanes`); any
    /// other module whose patch changed gets the defaults with that patch's
    /// saved values on top. Runs before every program build.
    fn prepare_lanes(&self) {
        self.seed_omni_lanes();
        let Ok(mut guard) = self.inner.state.lock() else {
            return;
        };
        let State { lanes, profile, .. } = &mut *guard;
        for (name, lane) in lanes.iter_mut() {
            if lane.omni_seed.is_some() {
                continue;
            }
            let def = profile
                .engines
                .iter()
                .flat_map(|e| &e.layers)
                .find(|l| &l.name == name);
            for (i, m) in lane.modules.iter_mut().enumerate() {
                if m.patch.is_empty() || m.macros_patch.as_deref() == Some(m.patch.as_str()) {
                    continue;
                }
                m.macros = default_macros();
                if let Some(saved) = def.and_then(|d| d.saved_macros(&m.patch, i as u32)) {
                    for v in saved {
                        set_macro_clamped(&mut m.macros, &v.id, v.value);
                    }
                }
                m.macros_patch = Some(m.patch.clone());
                lane.spans.clear();
            }
        }
    }

    /// Copy every module's knobs into the profile under the patch it holds,
    /// and save the profile a moment later (coalesced across a drag).
    fn remember_macros_soon(&self) {
        use std::sync::atomic::Ordering;
        {
            let Ok(mut guard) = self.inner.state.lock() else {
                return;
            };
            let State { lanes, profile, .. } = &mut *guard;
            for (name, lane) in lanes.iter() {
                let Some(def) = profile.layer_mut(name) else {
                    continue;
                };
                for (i, m) in lane.modules.iter().enumerate() {
                    if m.patch.is_empty() || m.macros_patch.as_deref() != Some(m.patch.as_str()) {
                        continue;
                    }
                    let values = m
                        .macros
                        .iter()
                        .filter(|(id, _)| Self::macro_is_dsp(id) || Self::macro_is_mod(id))
                        .map(|(id, v)| crate::profile::MacroValue {
                            id: id.clone(),
                            value: *v,
                        })
                        .collect();
                    def.remember_macros(&m.patch, i as u32, values);
                }
            }
            // Each scope's own settings (Tone, Limiter, FX Bypass).
            let own =
                |map: &BTreeMap<String, f32>, prefix: &str| -> Vec<crate::profile::MacroValue> {
                    map.iter()
                        .filter_map(|(k, v)| {
                            k.strip_prefix(prefix).map(|id| crate::profile::MacroValue {
                                id: id.to_string(),
                                value: *v,
                            })
                        })
                        .collect()
                };
            let State {
                lanes,
                engines,
                rig_globals,
                profile,
                ..
            } = &mut *guard;
            for (name, lane) in lanes.iter() {
                if let Some(def) = profile.layer_mut(name) {
                    def.scope_values = own(&lane.globals, LAYER);
                }
            }
            for e in &mut profile.engines {
                if let Some(state) = engines.get(&e.name) {
                    e.scope_values = own(&state.globals, ENGINE);
                }
            }
            profile.scope_values = own(rig_globals, RIG);
        }
        let r#gen = self.inner.save_gen.fetch_add(1, Ordering::AcqRel) + 1;
        let b = self.clone();
        let _ = std::thread::Builder::new()
            .name("keys-profile-save".into())
            .spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(1000));
                if b.inner.save_gen.load(Ordering::Acquire) != r#gen {
                    return; // a later edit owns the save
                }
                let profile = b.inner.state.lock().ok().map(|s| s.profile.clone());
                if let Some(p) = profile {
                    p.save();
                }
            });
    }

    fn profile_inputs(&self) -> Option<ProfileInputs> {
        self.prepare_lanes();
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
        let Some(lane) = lanes.get(layer) else {
            return signal_synth::engine::ModuleSettings::default();
        };
        let mut set =
            Self::settings_from_macros(lane, lane.modules.get(module).map(|m| &m.macros));
        // An Omnisphere lane: the knobs as seeded, through the same mapping,
        // so only what the player moved is applied onto the patch.
        if lane.omni_seed.is_some() {
            if let Some(base) = lane.omni_base.get(module) {
                set.baseline = Some(Box::new(Self::settings_from_macros(lane, Some(base))));
            }
        }
        set
    }

    /// A module's settings from its macro values (defaults for any unset).
    fn settings_from_macros(
        lane: &LaneState,
        macros: Option<&BTreeMap<String, f32>>,
    ) -> signal_synth::engine::ModuleSettings {
        let mut set = signal_synth::engine::ModuleSettings::default();
        let v = |id: &str| {
            macros
                .and_then(|m| m.get(id).copied())
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
        let fx_on = !lane.fx_gated;
        set.pan = v("source.pan");
        set.width = v("fx.width");
        set.transpose = v("source.transpose");
        set.fine = v("source.fine");
        set.keytrack = v("filter.keytrack");
        set.filter_drive = v("filter.drive");
        set.filter_mix = v("filter.mix");
        set.vib_rate = v("vib.rate");
        set.vib_depth = v("vib.depth");
        set.vib_delay_ms = v("vib.delay");
        set.warmth = v("tone.warmth");
        set.body = v("tone.body");
        set.drive = v("tone.drive");
        set.chorus = if fx_on { v("fx.chorus") } else { 0.0 };
        set.ambience = signal_synth::engine::AmbienceSettings {
            on: fx_on && v("amb.bypass") < 0.5,
            algo: v("amb.algo"),
            size: v("amb.size"),
            mix: v("amb.mix"),
            predelay_ms: v("amb.predelay"),
            decay: v("amb.decay"),
        };
        // The Effects page's Delay amount and the Delay section's mix are
        // one send: whichever is higher.
        // A note division (1..7) follows the band's tempo when there is one;
        // 0, or no tempo yet, is the free time in ms.
        const DIVS: [f32; 7] = [0.25, 0.5, 0.75, 1.0, 1.5, 2.0, 4.0];
        let div = v("dly.div").round() as usize;
        let time_ms = match (
            div.checked_sub(1).and_then(|i| DIVS.get(i)),
            signal_rig_host::tempo::get(),
        ) {
            (Some(factor), Some(bpm)) => 60_000.0 / bpm * factor,
            _ => v("dly.time"),
        };
        set.delay = signal_synth::engine::DelaySettings {
            on: fx_on && v("dly.bypass") < 0.5,
            style: v("dly.algo"),
            time_ms,
            feedback: v("dly.feedback"),
            mix: v("dly.mix").max(v("fx.delay")),
        };
        for (n, lfo) in set.lfos.iter_mut().enumerate() {
            let id = |k: &str| format!("lfo{}.{k}", n + 1);
            *lfo = (v(&id("rate")), v(&id("depth")), v(&id("shape")));
        }
        set.mod_env = (
            v("env3.attack"),
            v("env3.decay"),
            v("env3.sustain"),
            v("env3.release"),
        );
        set.mod_env2 = (
            v("env4.attack"),
            v("env4.decay"),
            v("env4.sustain"),
            v("env4.release"),
        );
        for (i, e) in ["env3", "env4"].into_iter().enumerate() {
            set.mod_env_dh[i] = (v(&format!("{e}.delay")), v(&format!("{e}.hold")));
            set.mod_env_depth[i] = v(&format!("{e}.depth"));
            set.mod_env_dest[i] =
                signal_synth::engine::ModDest::from_value(v(&format!("{e}.dest")));
        }
        for n in 0..4 {
            set.lfo_fade_ms[n] = v(&format!("lfo{}.fade", n + 1));
            set.lfo_dest[n] =
                signal_synth::engine::ModDest::from_value(v(&format!("lfo{}.dest", n + 1)));
        }
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

    /// Every distinct `.signalpack` the current profile's lanes reference,
    /// in lane order — the pack half of [`KeysRigSvc::lane_program_wire`].
    /// Non-pack specs (raw `library.styx` extractions, `.prt_omn` patches)
    /// are skipped: a browser can only stream built packs, and those lanes
    /// are exactly the ones that stay silent natively too.
    fn profile_pack_refs(&self) -> (Vec<KeysPackRef>, Vec<signal_keys_proto::KeysLaneRef>) {
        let Some((profile, index, _lanes)) = self.profile_inputs() else {
            return (Vec::new(), Vec::new());
        };
        (
            pack_refs_for(&profile, &index),
            lane_refs_for(&profile, &index),
        )
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
            for (name, lane) in &s.lanes {
                rig.set_lane_volume(Role::Layer, name, db_to_linear(lane.gain_db));
                rig.set_lane_mute(Role::Layer, name, lane.muted);
                rig.set_lane_solo(Role::Layer, name, lane.soloed);
                if let Some(cells) = rig.lane_cells(name) {
                    // Fold the lane's own audibility into its module cells.
                    //
                    // A lane/engine mute above is a daw TRACK op, applied
                    // after the lane's instrument has already rendered. The
                    // render tree never learns about it, so a muted lane goes
                    // on spawning voices and rendering them at full price for
                    // an output that is then multiplied by zero — and, when
                    // its pack cannot resolve a body, goes on logging a
                    // dead-key warning per note for an instrument nobody can
                    // hear. (Found via the Aux engine, which starts muted and
                    // holds Dolceola.)
                    //
                    // Zeroing the module cells puts that mute where the tree
                    // can act on it: `node_render`'s gain node then drops
                    // note-ons and stops rendering the subtree entirely. The
                    // track mute still applies underneath; zero times zero is
                    // the same silence, arrived at without the work.
                    let audible = if s.lane_is_audible(lane) { 1.0 } else { 0.0 };
                    // Modules are named "<layer> <slot>" in the lane's tree.
                    for i in 0..lane.modules.len() {
                        let module_name =
                            format!("{name} {}", signal_synth::engine::module_slot(i));
                        cells.set(Role::Module, &module_name, lane.module_gain(i) * audible);
                    }
                }
            }
            for (name, e) in &s.engines {
                rig.set_lane_volume(Role::Engine, name, db_to_linear(e.gain_db));
                rig.set_lane_mute(Role::Engine, name, e.muted);
            }
        } else {
            let cells = rig.gain_cells();
            // Address by (role, name), never name alone: a one-lane engine is
            // named after its lane ("Pad" holding "Pad"), and a flat name map
            // gave both the same cell — the engine's trim, written second,
            // put the lane's mute and solo straight back.
            for (name, lane) in &s.lanes {
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
        // Each scope's Tone and Limiter onto its track's stage.
        use signal_sampler::keys_rig::Scope;
        let scope = |map: &BTreeMap<String, f32>, prefix: &str| {
            let v = |k: &str| map.get(&format!("{prefix}{k}")).copied().unwrap_or(0.0);
            (
                v("tone.low"),
                v("tone.mid"),
                v("tone.high"),
                v("limiter") >= 0.5,
            )
        };
        let (l, m, h, lim) = scope(&s.rig_globals, RIG);
        rig.set_scope(&Scope::Rig, l, m, h, lim);
        for (name, e) in &s.engines {
            let (l, m, h, lim) = scope(&e.globals, ENGINE);
            rig.set_scope(&Scope::Engine(name.clone()), l, m, h, lim);
        }
        for (name, lane) in &s.lanes {
            let (l, m, h, lim) = scope(&lane.globals, LAYER);
            rig.set_scope(&Scope::Layer(name.clone()), l, m, h, lim);
        }
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
            .and_then(|r| r.as_ref().map(signal_sampler::KeysRig::is_lanes))
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
                seed_module_macros(&mut slot.macros, m);
                slot.macros_patch = Some(slot.patch.clone());
                // Omnisphere's LFOs are per-part, so every module of the
                // patch gets the same four.
                for (n, (rate, depth, shape)) in imported.lfos.iter().enumerate() {
                    let id = format!("lfo{}", n + 1);
                    set_macro_clamped(&mut slot.macros, &format!("{id}.rate"), *rate);
                    set_macro_clamped(&mut slot.macros, &format!("{id}.depth"), *depth);
                    set_macro_clamped(&mut slot.macros, &format!("{id}.shape"), *shape);
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
                                patch: lane.map(LaneState::primary_patch).unwrap_or_default(),
                                preset: lane.map(|l| l.preset.clone()).unwrap_or_default(),
                                gain_db: lane.map_or(0.0, |l| l.gain_db),
                                muted: lane.is_some_and(|l| l.muted),
                                exclude_global: layer.exclude_global,
                                soloed: lane.is_some_and(|l| l.soloed),
                                live: lane.is_some_and(LaneState::any_live),
                                key_lo: u32::from(layer.key_lo),
                                key_hi: u32::from(layer.key_hi),
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
                    tempo_bpm: st.tempo_bpm.max(0.0).round() as u32,
                })
                .collect(),
            active_stack: s.active_stack.map_or(u32::MAX, |i| i as u32),
            perform_mode: s.perform_mode,
            tempo_bpm: signal_rig_host::tempo::get().map_or(0, |b| b.round() as u32),
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
        let prefs = keys_audio_prefs();
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
                .map(std::string::ToString::to_string)
                .or_else(|| p.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "<non-string panic>".into());
            Err(eyre::eyre!("audio open panicked: {msg}"))
        });
        match opened {
            Ok(r) => {
                // Watch the audio callback: log whenever blocks miss their
                // deadline — "it runs out of buffer" left no trace before.
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(stats) = r.engine_stats() {
                    spawn_engine_watch(stats);
                }
                {
                    let mut rig = self
                        .inner
                        .rig
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    *rig = Some(r);
                }
                // A freshly opened rig has no MIDI input yet, and every
                // caller of ensure_open (start, preset load, rebuild) needs
                // one — attaching here makes this THE single open path, so a
                // rig can never end up audible-but-deaf.
                self.reattach_midi(signal_rig_host::midi::AttachTrigger::RigOpen);
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

    /// Point this rig at the shared MIDI hub.
    ///
    /// The rig no longer opens hardware itself: the hub holds one connection
    /// per port for the whole process and fans events out. Subscribing is
    /// cheap and synchronous — the expensive part (opening ports) happens
    /// once, in `rescan`, however many rigs are listening.
    fn reattach_midi(&self, trigger: signal_rig_host::midi::AttachTrigger) {
        let port = self
            .inner
            .state
            .lock()
            .ok()
            .and_then(|s| s.midi_port.clone());

        // Build the sink under the rig lock (cheap clones) and release the
        // lock before touching the hub, so a slow port open can never wedge
        // status() and with it the whole UI.
        let sink = {
            let rig = self
                .inner
                .rig
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match rig.as_ref() {
                Some(rig) => rig.midi_sink(),
                None => return, // Not running — nothing to feed.
            }
        };

        // The mod wheel also sweeps the pads' cutoff: note the value for the
        // worker and move on — the MIDI thread never waits on the rig state.
        let weak = Arc::downgrade(&self.inner);
        let sink = move |ev: midicore::TimedEvent| {
            if let midicore::MidiEvent::ControlChange { controller, value, .. } = &ev.event {
                if controller.get() == 1 {
                    if let Some(inner) = weak.upgrade() {
                        Self::queue_wheel(&inner, value.get());
                    }
                }
            }
            sink(ev);
        };
        let hub = signal_rig_host::midi_hub::hub();
        let sub = hub.subscribe("keys", port.clone(), sink);
        if let Ok(mut s) = self.inner.state.lock() {
            // Replacing the old subscription drops it, detaching the previous
            // sink — the rig must not be fed through a stale one after a
            // preset rebuild replaced its tracks.
            s.midi_handle = Some(sub);
        }
        let reopened = hub.rescan();

        tracing::info!(
            midi.rig = "keys",
            midi.trigger = trigger.as_str(),
            midi.selector = if port.is_some() { "named" } else { "omni" },
            midi.ports_seen = hub.ports().len(),
            midi.hub_reopened = reopened,
            "midi attach"
        );
        if hub.ports().is_empty() {
            tracing::warn!(
                midi.rig = "keys",
                "MIDI hub has no ports open — the rig is running but cannot be played"
            );
        }
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
        self.reattach_midi(signal_rig_host::midi::AttachTrigger::PortsChanged);
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
        let s = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
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
        let rt = if running {
            self.inner
                .rig
                .lock()
                .ok()
                .and_then(|r| r.as_ref().and_then(signal_sampler::KeysRig::engine_stats))
                .map(|st| {
                    use std::sync::atomic::Ordering::Relaxed;
                    let (last_ms, peak_ms) = st.render_ms();
                    signal_keys_proto::KeysRealtime {
                        xruns: st.xruns.load(Relaxed),
                        over_budget: st.over_budget.load(Relaxed),
                        mean_render_ms: st.mean_render_ms() as f32,
                        blocks: st.calls.load(Relaxed),
                        block_frames: st.block_frames.load(Relaxed),
                        peak_render_ms: peak_ms as f32,
                        render_ms: last_ms as f32,
                        // The artefact counters ride along with the deadline
                        // ones so a UI can show both. They answer different
                        // questions and the rig has been failing the second
                        // while passing the first.
                        ..artefacts()
                    }
                })
                .unwrap_or_default()
        } else {
            Default::default()
        };
        // `KeysRig::active_voices` has existed all along; the status reported
        // a hardcoded 0, which made the one number that explains a render
        // spike invisible to every UI and every test.
        let voices = if running {
            self.inner
                .rig
                .lock()
                .ok()
                .and_then(|r| r.as_ref().map(|r| r.active_voices() as u32))
                .unwrap_or(0)
        } else {
            0
        };
        // Enrich THIS span rather than logging: `status` is polled by every
        // attached UI, so the vox span architect already opens per call is a
        // free, regular carrier for the rig's realtime health — one wide
        // event per poll instead of a log line per metric.
        //
        // The audio thread contributes nothing here but relaxed atomic loads;
        // all of these are counters it bumped, read from this (ordinary)
        // thread. `wide::set` is a no-op when no OTel layer is installed, so
        // this costs nothing when nobody is collecting.
        //
        // `audio.gap_frames` is the field worth watching: it counts output
        // frames that were digitally silent while voices were sounding, which
        // is what a starved sample stream produces. Deadline counters
        // (`audio.over_budget`, `audio.xruns`) stay at zero through it,
        // because the callback IS on time — it is just rendering silence.
        {
            use architect_telemetry::wide;
            let g = signal_sampler::engine::output_glitches();
            wide::set("rig", "keys");
            wide::set("audio.running", running);
            wide::set("audio.voices", i64::from(voices));
            wide::set("audio.block_frames", i64::from(rt.block_frames));
            wide::set("audio.render_mean_ms", f64::from(rt.mean_render_ms));
            wide::set("audio.render_peak_ms", f64::from(rt.peak_render_ms));
            wide::set("audio.xruns", rt.xruns.cast_signed());
            wide::set("audio.over_budget", rt.over_budget.cast_signed());
            wide::set("audio.blocks", rt.blocks.cast_signed());
            wide::set("audio.gap_frames", g.gap_frames as i64);
            wide::set("audio.click_frames", g.click_frames as i64);
            wide::set("audio.nonfinite_frames", g.nonfinite_frames as i64);
            wide::set(
                "audio.notes_dropped",
                signal_sampler::engine::notes_dropped() as i64,
            );
        }
        KeysStatus {
            running,
            loaded_preset,
            master_peak,
            meters,
            voices,
            midi_port: s.midi_port.clone(),
            last_error: s.last_error.clone(),
            rt,
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
                b.do_load_preset(index as usize);
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

    fn lane_program_wire(&self) -> KeysLaneProgram {
        tracing::debug!("keys rpc: lane_program_wire →");
        let Some(program) = self.profile_lane_program() else {
            return KeysLaneProgram::default();
        };
        let wire = signal_sampler::keys_rig::WireProgram::from_program(&program);
        let program_json = match facet_json::to_string(&wire) {
            Ok(json) => json,
            Err(e) => {
                tracing::error!("keys rig: lane program wire serialize failed: {e}");
                return KeysLaneProgram::default();
            }
        };
        let (packs, lanes) = self.profile_pack_refs();
        // Wide event: enrich architect's per-RPC span — a browser boot that
        // fetched an empty or wrong program names its profile here.
        if let Some(profile) = self.inner.state.lock().ok().map(|s| s.profile.name.clone()) {
            architect_telemetry::wide::set("keys.profile", profile);
        }
        architect_telemetry::wide::set("keys.pack_count", packs.len() as i64);
        architect_telemetry::wide::set("keys.lane_count", lanes.len() as i64);
        KeysLaneProgram {
            program_json,
            packs,
            lanes,
        }
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

    fn reset_rt_peak(&self) {
        if let Ok(r) = self.inner.rig.lock() {
            if let Some(st) = r.as_ref().and_then(signal_sampler::KeysRig::engine_stats) {
                st.reset_peak();
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
        Self::queue_wheel(&self.inner, value.min(127) as u8);
    }

    fn midi_ports(&self) -> Vec<String> {
        KeysRig::midi_input_ports()
    }

    fn set_midi_port(&self, name: String) {
        if let Ok(mut s) = self.inner.state.lock() {
            s.midi_port = if name.is_empty() { None } else { Some(name) };
        }
        self.reattach_midi(signal_rig_host::midi::AttachTrigger::PortSelected);
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
        let (key_lo, key_hi) = s.profile.layer(&layer).map_or((0, 127), |(_, l)| {
            (u32::from(l.key_lo), u32::from(l.key_hi))
        });
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
                // Lit when it reaches sound (a live DSP parameter or the
                // level fader), not by a hand-kept flag that drifted.
                live: (Self::macro_is_dsp(def.id)
                    || def.id == "source.level"
                    || (Self::macro_is_mod(def.id)
                        && (lane.omni_seed.is_none() || Self::mod_reaches_import(def.id))))
                    && here.is_some_and(|m| !m.patch.is_empty()),
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
                let regate = Self::refresh_fx_gates(&mut s);
                drop(s);
                self.after_scope_param(regate);
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
            if Self::macro_rebuilds(target) {
                self.rebuild_soon();
            }
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
            engine,
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
                let regate = Self::refresh_fx_gates(&mut s);
                drop(s);
                self.after_scope_param(regate);
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
            if Self::macro_rebuilds(target) {
                self.rebuild_soon();
            }
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
                let regate = Self::refresh_fx_gates(&mut s);
                drop(s);
                self.after_scope_param(regate);
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
            if Self::macro_rebuilds(target) {
                self.rebuild_soon();
            }
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
        let reaches = Self::macro_is_dsp(def.id) || Self::macro_is_mod(def.id);
        if reaches
            && (!self.push_module_dsp(&layer, module as usize) || Self::macro_rebuilds(def.id))
        {
            self.rebuild_soon();
        }
        if reaches {
            self.remember_macros_soon();
        }
        self.publish_mixer();
    }

    fn set_drone(&self, engine: String, key: u32, octave: i32, playing: bool) {
        if let Ok(mut s) = self.inner.state.lock() {
            let est = s.engines.entry(engine).or_default();
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
            // A stack that knows its song's tempo sets it for the band.
            if stack.tempo_bpm > 0.0 {
                signal_rig_host::tempo::set(stack.tempo_bpm);
            }
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
            // The tempo playing now is this stack's song's.
            stack.tempo_bpm = signal_rig_host::tempo::get().unwrap_or(0.0);
        }
        // Scenes belong to the profile: save it.
        self.remember_macros_soon();
        self.publish_perform();
    }

    fn tap_tempo(&self) {
        let now = std::time::Instant::now();
        let bpm = {
            let Ok(mut taps) = self.inner.taps.lock() else {
                return;
            };
            tap_bpm(&mut taps, now)
        };
        if let Some(bpm) = bpm {
            signal_rig_host::tempo::set(bpm);
            tracing::info!("keys tap tempo: {bpm:.1} BPM");
        }
        self.publish_perform();
    }

    fn set_tempo(&self, bpm: u32) {
        if bpm > 0 {
            signal_rig_host::tempo::set(bpm as f32);
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

/// Every distinct `.signalpack` `profile`'s lanes reference, in lane order —
/// resolved through `index` (patch name → scanned spec path), skipping
/// non-pack specs (raw `library.styx` extractions, `.prt_omn` patches): a
/// browser can only stream built packs, and those lanes are exactly the ones
/// that stay silent natively too.
fn pack_refs_for(profile: &KeysProfile, index: &BTreeMap<String, PathBuf>) -> Vec<KeysPackRef> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for engine in &profile.engines {
        for layer in &engine.layers {
            for patch in layer.module_patches() {
                let Some(path) = index.get(&patch) else {
                    continue;
                };
                if !path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("signalpack"))
                {
                    continue;
                }
                let key = path.to_string_lossy().into_owned();
                if !seen.insert(key.clone()) {
                    continue;
                }
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
                    .to_string();
                out.push(KeysPackRef { key, name });
            }
        }
    }
    out
}

/// The program's lanes in order, each with the first streamable pack it
/// references (empty key = nothing to stream; the lane is silent natively
/// too, or purely synthesized).
fn lane_refs_for(
    profile: &KeysProfile,
    index: &BTreeMap<String, PathBuf>,
) -> Vec<signal_keys_proto::KeysLaneRef> {
    let mut out = Vec::new();
    for engine in &profile.engines {
        for layer in &engine.layers {
            let key = layer
                .module_patches()
                .into_iter()
                .filter_map(|p| index.get(&p))
                .find(|path| {
                    path.extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("signalpack"))
                })
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            out.push(signal_keys_proto::KeysLaneRef {
                engine: engine.name.clone(),
                name: layer.name.clone(),
                key,
            });
        }
    }
    out
}

const fn note_ev(note: u8, velocity: u8) -> MidiEvent {
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
                merged.adopt_saved_macros(&saved);
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
    // The Keyscape packs (one self-contained pack per instrument, preferred
    // over the raw extraction).
    let packs_root = pack_root("FTS_KEYSCAPE_PACKS", "Keys/Keyscape/Packs");
    let (mut packs, mut pack_specs) = scan_packs(&packs_root);
    // One engine, one library: the Omnisphere soundsources are loadable into
    // any lane exactly like a Keyscape pack (they're both just sources for
    // the Signal Engine's Soundsource block).
    let omni_root = pack_root("FTS_OMNISPHERE_PACKS", "Keys/Omnisphere/Packs");
    let (omni, omni_specs) = scan_packs_recursive_as(&omni_root, "Soundsource", "module", "Synth");
    packs.extend(omni);
    pack_specs.extend(omni_specs);
    // The NI Essential Pianos. A pack is a whole lane's worth of instrument
    // (the Piano packs) or its pedal-down resonance layer, which loads on its
    // own so a tight-memory rig can leave it out — see `ni-pianos.styx`.
    let ni_root = pack_root("FTS_NI_PIANO_PACKS", "Full/Keys");
    let (ni, ni_specs) = scan_packs_recursive_as(&ni_root, "Grand", "layer", "Keys");
    tracing::info!(packs = ni.len(), "keys rig: NI piano packs");
    packs.extend(ni);
    pack_specs.extend(ni_specs);
    // Authored Omnisphere patches — these open into a whole layer.
    let patch_root = sampled_path("FTS_OMNISPHERE_PATCHES", OMNISPHERE_PATCHES_REL);
    let (patches, patch_specs) = scan_omni_patches(&patch_root);
    tracing::info!(patches = patches.len(), "keys rig: omnisphere patches");
    packs.extend(patches);
    pack_specs.extend(patch_specs);
    if !packs.is_empty() {
        return (packs, pack_specs);
    }
    let root = sampled_path("FTS_KEYSCAPE_ROOT", "Keys/Keyscape");
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
const OMNISPHERE_PATCHES_REL: &str =
    "Synth/Spectrasonics-Patches/Omnisphere/Settings Library/Patches";

/// Enumerate `.prt_omn` patches under `root` — the **module presets**: an
/// authored voice (source + filter + envelopes + unison) that loads onto a
/// module, spilling onto the next ones when the patch has several layers.
fn scan_omni_patches(root: &str) -> (Vec<KeysPreset>, Vec<PathBuf>) {
    let mut presets: Vec<KeysPreset> = Vec::new();
    let mut specs: Vec<PathBuf> = Vec::new();
    // Display name → slot. Factory names collapse once the library prefix is
    // stripped ("KEY │ American Obesity", "AV │ American Obesity"), and a
    // user's saved copy of a patch shares its factory name — the gig's
    // "Worship Gig 3" patches are edited copies of factory ones. A patch
    // under `User/` is the player's version, so it shadows the factory one;
    // otherwise the first found stays.
    let mut slot: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let is_user = |p: &std::path::Path| p.components().any(|c| c.as_os_str() == "User");
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
            let preset = KeysPreset {
                kind: "Patch".into(),
                name: display.clone(),
                loaded: false,
                scope: "layer".into(),
                tags,
                variants: crate::variations::variation_names(name),
            };
            match slot.get(&display) {
                Some(&i) => {
                    if is_user(&patch) && !is_user(&specs[i]) {
                        presets[i] = preset;
                        specs[i] = patch;
                    }
                }
                None => {
                    slot.insert(display, presets.len());
                    presets.push(preset);
                    specs.push(patch);
                }
            }
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
// (`Full/Keys` in the pack library — see `pack_root`.)

/// Root of the built Omnisphere soundsource packs — the synth half of the
/// shared library. Override with `FTS_OMNISPHERE_PACKS`.
// (`Keys/Omnisphere/Packs` in the pack library — see `pack_root`.)

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
const fn is_drone(engine: &str) -> bool {
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

    /// The rig starts with the pads and synths audible — a muted ENGINE hides
    /// its lanes however they are switched — and Club Europa muted.
    #[test]
    fn the_worship_rig_starts_with_the_pads_on_and_club_europa_muted() {
        let mut s = State::default();
        s.adopt_profile(worship_profile());
        for engine in ["Keys", "Pad", "Bass", "Aux"] {
            assert!(!s.engines.get(engine).expect("engine").muted, "{engine} starts muted");
        }
        for lane in ["Pad", "Shimmer", "Synth 1"] {
            assert!(!s.lanes.get(lane).expect("lane").muted, "{lane} starts muted");
        }
        assert!(s.lanes.get("Synth 2").expect("lane").muted, "Club Europa starts audible");
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

    // ── The browser boot payload (lane_program_wire's two halves) ────────

    /// The worship profile's lane program survives the `WireProgram` JSON
    /// round trip — the exact bytes `lane_program_wire` serves and the keys
    /// worklet's `openLanes` parses.
    #[test]
    fn lane_program_wire_roundtrips_through_json() {
        let profile = worship_profile();
        let program = profile.build_lane_program(
            |patch| Some(format!("/packs/{patch}.signalpack")),
            |_, _| Default::default(),
        );
        let wire = signal_sampler::keys_rig::WireProgram::from_program(&program);
        let json = facet_json::to_string(&wire).expect("wire program serializes");
        let back: signal_sampler::keys_rig::WireProgram =
            facet_json::from_str(&json).expect("wire program parses back");
        let back = back.into_lane_program();
        assert_eq!(back.name, program.name);
        let shape = |p: &signal_sampler::keys_rig::LaneProgram| -> Vec<(String, Vec<String>)> {
            p.engines
                .iter()
                .map(|e| {
                    (
                        e.name.clone(),
                        e.layers.iter().map(|l| l.name.clone()).collect(),
                    )
                })
                .collect()
        };
        assert_eq!(shape(&back), shape(&program));
        assert_eq!(back.tail.is_some(), program.tail.is_some());
    }

    /// The pack manifest lists each referenced `.signalpack` once (spec-path
    /// key + file-stem name) and skips specs a browser cannot stream.
    #[test]
    fn pack_refs_dedup_and_skip_non_packs() {
        let profile = worship_profile();
        let patches: Vec<String> = profile
            .engines
            .iter()
            .flat_map(|e| e.layers.iter())
            .flat_map(super::super::profile::LayerDef::module_patches)
            .filter(|p| !p.is_empty())
            .collect();
        assert!(patches.len() > 2, "worship profile references patches");
        let mut index: BTreeMap<String, PathBuf> = BTreeMap::new();
        // First patch resolves to a non-pack spec; the rest to packs.
        index.insert(
            patches[0].clone(),
            PathBuf::from(format!("/specs/{}.prt_omn", patches[0])),
        );
        for p in &patches[1..] {
            index.insert(p.clone(), PathBuf::from(format!("/packs/{p}.signalpack")));
        }
        let refs = pack_refs_for(&profile, &index);
        assert!(!refs.is_empty());
        assert!(
            refs.iter().all(|r| r.key.ends_with(".signalpack")),
            "only built packs are streamable"
        );
        assert!(
            !refs.iter().any(|r| r.key.contains(&patches[0])),
            "the non-pack spec is skipped"
        );
        let mut keys: Vec<&str> = refs.iter().map(|r| r.key.as_str()).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), refs.len(), "each pack listed once");
        for r in &refs {
            assert_eq!(r.key, format!("/packs/{}.signalpack", r.name));
        }
    }
}

/// The keys rig's saved audio settings (`rigs/keys-rig.styx`, the guitar
/// rig's format: device, rate, buffer), as an output-only engine's prefs —
/// a synth generates, it has no input to open. Unsaved: the system output at
/// 48 kHz and 256 frames. On iOS the fixed-size request rides a macOS-only
/// CoreAudio property (AVAudioSession owns the IO buffer there), so the
/// backend default is asked for instead.
fn keys_audio_prefs() -> AudioIoPrefs {
    let mgr = signal_sampler::rig_manager::RigManager::load(KEYS_RIG_NAME);
    let mut prefs = AudioIoPrefs::from(&mgr.audio);
    prefs.want_input = false;
    prefs.input_device = String::new();
    prefs.phones_routing = false;
    if cfg!(target_os = "ios") {
        prefs.buffer_size = 0;
    }
    prefs
}

/// The keys rig's settings name (`rigs/keys-rig.styx`).
pub const KEYS_RIG_NAME: &str = "Keys Rig";

/// Log the audio callback's health every 2 s while it misbehaves: blocks
/// over their realtime budget and driver xruns since the last report, the
/// peak and mean render time against the budget. Quiet while it keeps up.
#[cfg(not(target_arch = "wasm32"))]
fn spawn_engine_watch(stats: std::sync::Arc<daw_audio_io::duplex::EngineStats>) {
    use std::sync::atomic::Ordering::Relaxed;
    let weak = std::sync::Arc::downgrade(&stats);
    drop(stats);
    let _ = std::thread::Builder::new()
        .name("keys-engine-watch".into())
        .spawn(move || {
            let (mut over0, mut xrun0, mut calls0, mut total0) = (0u64, 0u64, 0u64, 0u64);
            let skips = daw::standalone::audio_engine::render::plugin_stage_skips;
            let mut skip0 = skips();
            let mut cut0 = signal_sampler::keys_rig::guard_cuts();
            loop {
                std::thread::sleep(std::time::Duration::from_secs(2));
                let Some(st) = weak.upgrade() else {
                    return;
                };
                let (over, xruns) = (st.over_budget.load(Relaxed), st.xruns.load(Relaxed));
                let (calls, total) = (st.calls.load(Relaxed), st.total_render_ns.load(Relaxed));
                let frames = st.block_frames.load(Relaxed).max(1);
                let budget_ms = f64::from(frames) / 48.0;
                let peak_ms = st.peak_render_ns.swap(0, Relaxed) as f64 / 1e6;
                let mean_ms = if calls > calls0 {
                    (total - total0) as f64 / (calls - calls0) as f64 / 1e6
                } else {
                    0.0
                };
                let skipped = skips();
                let cuts = signal_sampler::keys_rig::guard_cuts();
                if over > over0 || xruns > xrun0 || skipped > skip0 || cuts > cut0 {
                    tracing::warn!(
                        late_blocks = over - over0,
                        xruns = xruns - xrun0,
                        // Blocks rendered with no instruments: the plugin map
                        // was held by a control thread when the block began.
                        silent_blocks = skipped - skip0,
                        // Held notes the CPU guard cut to keep up.
                        guard_cuts = cuts - cut0,
                        voices = signal_sampler::keys_rig::total_voices(),
                        peak_ms = format!("{peak_ms:.2}"),
                        mean_ms = format!("{mean_ms:.2}"),
                        budget_ms = format!("{budget_ms:.2}"),
                        block = frames,
                        "keys audio: under strain"
                    );
                }
                (over0, xrun0, calls0, total0, skip0, cut0) = (over, xruns, calls, total, skipped, cuts);
            }
        });
}

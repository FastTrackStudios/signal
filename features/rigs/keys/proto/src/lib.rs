//! Keys-rig wire contract — the detachable-GUI boundary for the Nord-style
//! keys rig. The rig core hosts a composition tree (engines → layers → sampler
//! blocks) on the shared engine; every front-end is a vox remote speaking the
//! [`keys::KeysRig`] service.
//!
//! All types are plain `facet::Facet` data (no Dioxus, no audio backend), so
//! this crate compiles for wasm + embedded.

use facet::Facet;

// ── Wire types ──────────────────────────────────────────────────────────────

/// One loadable keys preset (a Keyscape instrument / composition program).
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct KeysPreset {
    /// Display name ("LA Custom C7 Grand", "Rhodes - Classic", …).
    pub name: String,
    /// Broad category for grouping ("Grand", "Rhodes", "Wurlitzer", …).
    pub kind: String,
    /// Whether this is the currently-loaded preset.
    pub loaded: bool,
    /// The deepest level this preset makes sense at: `"engine"` (a whole
    /// instrument program), `"layer"` (what one lane plays) or `"module"` (a
    /// single soundsource inside a lane). The browser shows a level's presets
    /// **and everything below it** — a soundsource is a legitimate thing to
    /// load into a layer, it just fills one module of it.
    pub scope: String,
    /// Engines this preset belongs to ("Keys", "Synth", "Organ", "Pad") — the
    /// browser filters by the selected engine's tag, so picking a lane in Keys
    /// never shows you Omnisphere pads.
    pub tags: Vec<String>,
    /// **Variations on this preset.** A library entry is a default with
    /// alternatives behind it — "C7 Grand" is the preset, "Felt", "Close
    /// Mic'd", "Lid Down" are variations of it. The browser navigates into
    /// them; loading the entry itself loads the default. Empty until a pack
    /// authors variations.
    pub variants: Vec<String>,
}

/// A node in the loaded composition tree (engine → layers → blocks) — the
/// structure the control view renders as selectable boxes.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct KeysNode {
    /// Path id addressing this node (`"keys"`, `"keys/layer-a"`, …).
    pub id: String,
    /// Display label ("Keys", "Layer A", "Piano", …).
    pub label: String,
    /// Node role: "preset" | "engine" | "layer" | "module" | "block".
    pub role: String,
    /// Whether this node currently produces sound (has a live backend).
    pub live: bool,
    /// Child nodes, in order.
    pub children: Vec<KeysNode>,
}

// ── Mixer (engines → layers) ────────────────────────────────────────────────

/// One layer lane in the live mixer — the thing a fader rides and a patch
/// loads into.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct KeysLayerModel {
    /// Layer container name ("Keys A", "Organ B", …) — the fader's address.
    pub name: String,
    /// The engine this lane belongs to.
    pub engine: String,
    /// What the lane is playing: the module preset it was opened from, else
    /// module A's soundsource; empty = an empty lane.
    pub patch: String,
    /// The module preset behind that name, when there is one.
    pub preset: String,
    /// Fader position (dB).
    pub gain_db: f32,
    pub muted: bool,
    pub soloed: bool,
    /// The lane has a sounding backend (its patch resolved and loaded).
    pub live: bool,
    /// Key window — `0..=127` is the whole keyboard.
    pub key_lo: u32,
    pub key_hi: u32,
    /// The lane's modules — what the engine zoom shows a fader per.
    pub modules: Vec<KeysModule>,
}

/// One engine — an instrument part holding parallel layers.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct KeysEngineModel {
    pub name: String,
    /// The engine fader (dB) — rides all its layers.
    pub gain_db: f32,
    pub muted: bool,
    pub layers: Vec<KeysLayerModel>,
}

/// The whole mixer: what the Control view renders.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct KeysMixer {
    /// Active profile name.
    pub profile: String,
    pub engines: Vec<KeysEngineModel>,
    /// Master output trim (dB).
    pub master_db: f32,
}

// ── Layer zoom (the play surface for one lane) ──────────────────────────────

/// One controllable macro on a layer — the knobs the zoomed-in layer view
/// renders, grouped into panels (Filter, Amp Env, Vibrato, …).
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct KeysMacro {
    /// Stable id, unique within the layer ("filter.cutoff").
    pub id: String,
    /// Display name ("Cutoff").
    pub name: String,
    /// Panel this macro belongs to ("Filter") — see the engine's macro groups.
    pub group: String,
    pub value: f32,
    pub min: f32,
    pub max: f32,
    /// Display unit ("Hz", "ms", "%", "st"); empty for a bare number.
    pub unit: String,
    /// Whether the parameter currently reaches DSP. The engine's block stack
    /// is placeholder-first, so a macro can be authored + persisted before its
    /// block has an implementation — the UI dims those instead of lying.
    pub live: bool,
    /// A layer macro whose modules disagree: the knob is a **bipolar offset**
    /// (`min`/`max` are −1..1, centre = the modules' own settings) instead of
    /// an absolute value. Omnisphere's Global Controls behave the same way —
    /// unipolar and 1:1 while the layers match, scaling around the patch's
    /// values once they differ.
    pub bipolar: bool,
    /// Layer macros only: what the modules beneath currently hold, as text
    /// ("0.4–2.1 kHz" / "1.6 s"), so a bipolar knob can still say what it is.
    pub spread: String,
}

/// An ADSR in macro units — milliseconds and 0..1 sustain.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct KeysEnv {
    pub attack_ms: f32,
    pub decay_ms: f32,
    pub sustain: f32,
    pub release_ms: f32,
}

/// One module inside a layer — the engine instance the zoom edits.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct KeysModule {
    /// 0-based slot index within the layer.
    pub index: u32,
    /// Slot label ("A".."D").
    pub slot: String,
    /// The soundsource loaded in this module's Source Block ("OB-8 PWM Big
    /// Strings"); empty = silent. A module IS its source — the preset name
    /// belongs to the layer that opened it.
    pub patch: String,
    /// The module has a realized source.
    pub live: bool,
    /// The module's own fader (dB) — modules sum inside the layer.
    pub gain_db: f32,
    /// Switched off: silent, but its source and settings are kept.
    pub enabled: bool,
    /// The module's amp (ENV 1) and filter (ENV 2) envelopes, so the layer
    /// can draw every module's shape on one pair of axes.
    pub amp_env: KeysEnv,
    pub filter_env: KeysEnv,
    /// The module's filter, for the layer's overlay: cutoff (Hz) + resonance.
    pub cutoff_hz: f32,
    pub resonance: f32,
}

/// Everything the layer-zoom view needs for one lane.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct KeysLayerDetail {
    pub layer: String,
    pub engine: String,
    /// The layer's four modules (Omnisphere's Quadzone) — what A/B/C/D
    /// switches between.
    pub modules: Vec<KeysModule>,
    /// Which module the `macros`, `patch` and `tree` below describe.
    pub module: u32,
    /// The SELECTED module's soundsource.
    pub patch: String,
    /// The module preset this layer was opened from ("American Obesity"),
    /// empty when its modules were assembled by hand.
    pub preset: String,
    pub gain_db: f32,
    pub muted: bool,
    pub key_lo: u32,
    pub key_hi: u32,
    /// The SELECTED MODULE's macros, in panel order.
    pub macros: Vec<KeysMacro>,
    /// The LAYER's macros — Omnisphere's Global Controls: one Filter,
    /// Envelope, Vibrato, Unison, Ambience, Tone and Effects surface that
    /// drives every audible module beneath (see `set_layer_global`).
    pub layer_macros: Vec<KeysMacro>,
    /// The lane's block tree — the Signal Engine program it's running.
    pub tree: KeysNode,
}

// ── Performance (stacks / scenes) ───────────────────────────────────────────

/// One footswitch stack — a named scene over the mixer.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct KeysStack {
    pub name: String,
    /// One-line description of the sound.
    pub blurb: String,
    /// This stack's scene is the one currently applied.
    pub is_active: bool,
}

/// The live performance model: the profile's stacks + grid mode.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct KeysPerform {
    pub profile_name: String,
    pub stacks: Vec<KeysStack>,
    /// Index of the active stack, or `u32::MAX` when none has been pressed.
    pub active_stack: u32,
    /// Grid mode: 0 Preset (browse the library), 1 Profile (stacks),
    /// 2 Setlist (song-adaptive) — mirrors the guitar rig's modes.
    pub perform_mode: u32,
}

/// Live transport + meter snapshot — the high-rate poll payload.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct KeysStatus {
    pub running: bool,
    /// The loaded preset's display name, if any.
    pub loaded_preset: Option<String>,
    /// Master output peak (linear 0..~1).
    pub master_peak: f32,
    /// Active voices.
    pub voices: u32,
    /// The attached MIDI input port name, if any (None = omni / all).
    pub midi_port: Option<String>,
    /// The last audio-open / preset-load failure, if the engine isn't
    /// running because of one (surfaced by phone UIs with no log access).
    pub last_error: Option<String>,
}

pub mod keys {
    //! Live keys-rig control. `KeysRig` → `KeysRigClient` / `KeysRigService` /
    //! `keys_rig_serve`, plus the `#[subscribe]` stream sibling.

    use facet::Facet;

    use super::{
        KeysLayerDetail, KeysMixer, KeysNode, KeysPerform, KeysPreset, KeysStatus,
    };
    // `KeysModule` rides inside `KeysLayerDetail`.

    /// One live rig change. Every variant carries full state (idempotent
    /// re-application) so a reconnecting subscriber is correct after the next
    /// event of each kind.
    #[derive(Clone, Debug, PartialEq, Facet)]
    #[repr(C)]
    pub enum KeysEvent {
        /// Transport + meters (published at meter rate while running).
        Status(KeysStatus),
        /// The available presets changed (library scan).
        Library(Vec<KeysPreset>),
        /// The loaded composition tree changed (its engine/layer structure).
        Tree(KeysNode),
        /// The mixer changed (fader / mute / patch assignment).
        Mixer(KeysMixer),
        /// The performance model changed (stack pressed, mode switched).
        Perform(KeysPerform),
        /// Recent MIDI activity (oldest first), for the monitor.
        Midi(Vec<midicore_proto::MidiEvent>),
    }

    #[architect::rpc]
    pub trait KeysRig {
        /// (Re-)open the audio device. Returns immediately; open happens
        /// off-thread.
        fn start(&self);
        /// Close the audio device.
        fn stop(&self);
        /// Live transport + meter snapshot.
        fn status(&self) -> KeysStatus;
        /// Every preset in the library (the browser).
        fn presets(&self) -> Vec<KeysPreset>;
        /// Re-scan the pack library (after a download added packs) and
        /// publish the updated preset list.
        fn rescan(&self);
        /// Load preset `index` from [`presets`](Self::presets).
        fn load_preset(&self, index: u32);
        /// The loaded composition tree (engine → layers → blocks).
        fn tree(&self) -> KeysNode;

        // ── Mixer ───────────────────────────────────────────────────────
        /// The live mixer: engines, their layers, faders and patches.
        fn mixer(&self) -> KeysMixer;
        /// Ride a layer's fader (dB). Live — no rebuild, no audio gap.
        fn set_layer_gain(&self, layer: String, db: f32);
        /// Ride an engine's fader (dB) — scales all its layers.
        fn set_engine_gain(&self, engine: String, db: f32);
        /// Master output trim (dB).
        fn set_master_gain(&self, db: f32);
        /// Mute / unmute one layer (its fader position is remembered).
        fn set_layer_mute(&self, layer: String, muted: bool);
        /// Mute / unmute a whole engine.
        fn set_engine_mute(&self, engine: String, muted: bool);
        /// Solo a layer (any solo silences every un-soloed lane).
        fn set_layer_solo(&self, layer: String, soloed: bool);
        /// Load preset `preset` (from [`presets`](Self::presets)) into one
        /// MODULE of `layer` — the source IS that module's Source Block, so
        /// this is the engine's normal load path.
        fn set_layer_patch(&self, layer: String, module: u32, preset: u32);
        /// Empty one module (silences it and frees its samples).
        fn clear_layer(&self, layer: String, module: u32);
        /// Ride one module's fader (dB) — live, no rebuild.
        fn set_module_gain(&self, layer: String, module: u32, db: f32);
        /// Switch a module on/off (its source stays loaded).
        fn set_module_enabled(&self, layer: String, module: u32, on: bool);

        // ── Layer zoom ──────────────────────────────────────────────────
        /// Everything the zoomed-in layer view renders for one module:
        /// its source, macros and slice of the engine program.
        fn layer_detail(&self, layer: String, module: u32) -> KeysLayerDetail;
        /// Set one macro on one module of a layer.
        fn set_layer_macro(&self, layer: String, module: u32, id: String, value: f32);

        /// Move a LAYER macro — the Global Control. Absolute while the
        /// audible modules agree on the parameter, a bipolar offset that
        /// scales them around their own settings once they don't.
        fn set_layer_global(&self, layer: String, id: String, value: f32);

        // ── Performance ─────────────────────────────────────────────────
        /// The performance model: stacks + grid mode.
        fn perform(&self) -> KeysPerform;
        /// Press a footswitch stack — applies its scene across the mixer.
        fn press_stack(&self, index: u32);
        /// Grid mode: 0 Preset, 1 Profile (stacks), 2 Setlist.
        fn set_perform_mode(&self, mode: u32);
        /// Store the mixer's current state into stack `index` (write the
        /// scene from what you're hearing).
        fn capture_stack(&self, index: u32);
        /// Trigger a note from the UI (velocity 0 = note-off).
        fn trigger(&self, note: u32, velocity: u32);
        /// Enumerate hardware MIDI input ports.
        fn midi_ports(&self) -> Vec<String>;
        /// Attach a hardware MIDI input by name (empty = omni / all inputs).
        fn set_midi_port(&self, name: String);
        /// Recent MIDI events seen by the core (oldest first), for the monitor.
        fn midi_recent(&self) -> Vec<midicore_proto::MidiEvent>;

        /// Every rig change, as it happens.
        #[subscribe]
        fn events(&self) -> KeysEvent;
    }
}

pub use keys::{KeysEvent, KeysRig};

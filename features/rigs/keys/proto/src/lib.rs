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
    /// Patch loaded in the lane (a pack / library stem); empty = an empty lane.
    pub patch: String,
    /// Fader position (dB).
    pub gain_db: f32,
    pub muted: bool,
    pub soloed: bool,
    /// The lane has a sounding backend (its patch resolved and loaded).
    pub live: bool,
    /// Key window — `0..=127` is the whole keyboard.
    pub key_lo: u32,
    pub key_hi: u32,
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

    use super::{KeysMixer, KeysNode, KeysPerform, KeysPreset, KeysStatus};

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
        /// Load preset `preset` (from [`presets`](Self::presets)) into
        /// `layer`. Rebuilds that lane — the patch IS the Sampler block's
        /// spec, so this is the block/module system's normal load path.
        fn set_layer_patch(&self, layer: String, preset: u32);
        /// Empty a layer (silences the lane and frees its samples).
        fn clear_layer(&self, layer: String);

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

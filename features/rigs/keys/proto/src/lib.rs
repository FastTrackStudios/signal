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
}

pub mod keys {
    //! Live keys-rig control. `KeysRig` → `KeysRigClient` / `KeysRigService` /
    //! `keys_rig_serve`, plus the `#[subscribe]` stream sibling.

    use facet::Facet;

    use super::{KeysNode, KeysPreset, KeysStatus};

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

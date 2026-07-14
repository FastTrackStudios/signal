//! Synth-rig wire contract — the detachable-GUI boundary for the software-synth
//! rig (Omnisphere patch import). Sibling of `signal-keys-proto`: the rig core
//! hosts a composition tree (Quadzone → layers → oscillator/filter/amp/FX) on
//! the shared engine; every front-end is a vox remote speaking the
//! [`synth::SynthRig`] service.
//!
//! All types are plain `facet::Facet` data (no Dioxus, no audio backend), so
//! this crate compiles for wasm + embedded.

use facet::Facet;

// ── Wire types ──────────────────────────────────────────────────────────────

/// One loadable synth preset (an imported Omnisphere `.prt_omn` / `.mlt_omn`).
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct SynthPreset {
    /// Display name (the patch file's stem).
    pub name: String,
    /// Broad category for grouping ("Pads", "Bass", "Lead", "Poly", …) — the
    /// patch's library folder.
    pub kind: String,
    /// Whether this is the currently-loaded preset.
    pub loaded: bool,
}

/// A node in the loaded composition tree (Quadzone → layers → blocks) — the
/// structure the control view renders as selectable boxes.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct SynthNode {
    /// Path id addressing this node (`"omnisphere"`, `"omnisphere/layer-a"`, …).
    pub id: String,
    /// Display label ("Omnisphere", "Layer A", "Filter 1", …).
    pub label: String,
    /// Node role: "preset" | "engine" | "layer" | "module" | "block".
    pub role: String,
    /// Whether this node currently produces sound (has a live backend).
    pub live: bool,
    /// Child nodes, in order.
    pub children: Vec<SynthNode>,
}

/// Live transport + meter snapshot — the high-rate poll payload.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct SynthStatus {
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

pub mod synth {
    //! Live synth-rig control. `SynthRig` → `SynthRigClient` / `SynthRigService`
    //! / `synth_rig_serve`, plus the `#[subscribe]` stream sibling.

    use facet::Facet;

    use super::{SynthNode, SynthPreset, SynthStatus};

    /// One live rig change. Every variant carries full state (idempotent
    /// re-application) so a reconnecting subscriber is correct after the next
    /// event of each kind.
    #[derive(Clone, Debug, PartialEq, Facet)]
    #[repr(C)]
    pub enum SynthEvent {
        /// Transport + meters (published at meter rate while running).
        Status(SynthStatus),
        /// The available presets changed (library scan).
        Library(Vec<SynthPreset>),
        /// The loaded composition tree changed (its layer/block structure).
        Tree(SynthNode),
        /// Recent MIDI activity (oldest first), for the monitor.
        Midi(Vec<midicore_proto::MidiEvent>),
    }

    #[architect::rpc]
    pub trait SynthRig {
        /// (Re-)open the audio device. Returns immediately; open happens
        /// off-thread.
        fn start(&self);
        /// Close the audio device.
        fn stop(&self);
        /// Live transport + meter snapshot.
        fn status(&self) -> SynthStatus;
        /// Every preset in the library (the browser).
        fn presets(&self) -> Vec<SynthPreset>;
        /// Load preset `index` from [`presets`](Self::presets).
        fn load_preset(&self, index: u32);
        /// The loaded composition tree (layers → blocks).
        fn tree(&self) -> SynthNode;
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
        fn events(&self) -> SynthEvent;
    }
}

pub use synth::{SynthEvent, SynthRig};

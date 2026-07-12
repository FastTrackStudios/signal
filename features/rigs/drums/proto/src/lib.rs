//! Drum-rig wire contract — the detachable-GUI boundary.
//!
//! The rig core is 100% headless: it loads a GGD-style `.signalpreset` kit onto
//! the shared sampler, plays it from MIDI (hardware e-kit through a drum-map
//! converter, or UI pads), and exposes the multi-mic drum mixer. Every
//! front-end (desktop Blitz window, browser wasm, plugin editor) is a *remote*
//! that speaks the [`drum::DrumRig`] service over a vox link.
//!
//! All types are plain `facet::Facet` data — no Dioxus, no audio backend — so
//! this crate compiles for wasm and embedded.

use facet::Facet;

// ── Wire types ────────────────────────────────────────────────────────────

/// One selectable kit (a `.signalpreset` in the library).
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct KitInfo {
    /// Display name (the preset `name`, else the file stem).
    pub name: String,
    /// Absolute path to the `.signalpreset`.
    pub path: String,
    /// Whether this is the currently-loaded kit.
    pub loaded: bool,
}

/// One drum piece in the loaded kit (an engine + its trigger note).
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct PieceInfo {
    /// Engine id within the kit (e.g. "kick", "snare-a").
    pub id: String,
    /// The GM note that triggers this piece (first routed note).
    pub note: u32,
    /// Number of samples resident / total (preload progress).
    pub loaded_samples: u32,
    pub total_samples: u32,
}

/// The mixer-strip kind (drum mixer: close-mic channels direct to master,
/// overhead/room mics as sends into shared buses).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Facet)]
#[repr(u8)]
pub enum StripKind {
    /// A close-mic channel, direct to master.
    Channel,
    /// A mic send into a bus.
    Send,
    /// A shared bus (overhead / room), summing sends to master.
    Bus,
}

impl Default for StripKind {
    fn default() -> Self {
        StripKind::Channel
    }
}

/// One mixer strip (channel / send / bus) in the drum mixer.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct MixerStrip {
    pub kind: StripKind,
    /// Index within its own kind's list (addresses set_* calls).
    pub idx: u32,
    /// Display label.
    pub label: String,
    pub gain_db: f32,
    pub muted: bool,
    pub soloed: bool,
    /// Current peak level (linear 0..~1).
    pub peak: f32,
}

/// Which physical drum-map the attached hardware sends (so the converter knows
/// how to remap it onto the loaded kit). Mirrors `midicore::DrumMap`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Facet)]
#[repr(u8)]
pub enum InputMap {
    /// No conversion — hardware already speaks the kit's note layout.
    Direct,
    /// Alesis Strata Prime e-kit.
    StrataPrime,
    /// FTS internal drum map.
    Fts,
    /// GGD v1 ("Halpern") layout.
    Ggd,
}

impl Default for InputMap {
    fn default() -> Self {
        InputMap::Direct
    }
}

/// Live transport + meter snapshot — the high-rate poll payload.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct DrumStatus {
    /// Audio device open and processing.
    pub running: bool,
    /// The loaded kit's display name, if any.
    pub loaded_kit: Option<String>,
    /// Master output peak (linear 0..~1).
    pub master_peak: f32,
    /// Active voices across the kit.
    pub voices: u32,
    /// The attached MIDI input port name, if any.
    pub midi_port: Option<String>,
    /// The hardware input map currently applied.
    pub input_map: InputMap,
    /// Kit preload progress (0..1); < 1 while samples still decode.
    pub preload: f32,
}

// ── Services ──────────────────────────────────────────────────────────────

pub mod drum {
    //! Live drum-rig control. `DrumRig` → `DrumRigClient` / `DrumRigService` /
    //! `drum_rig_serve`, plus the `#[subscribe]` stream sibling
    //! (`DrumRigStreamService`, `DrumRigStreamSource`).

    use facet::Facet;

    use super::{DrumStatus, InputMap, KitInfo, MixerStrip, PieceInfo};

    /// One live rig change. Every variant carries **full state** (idempotent
    /// re-application) so a late/reconnecting subscriber is correct after the
    /// next event of each kind.
    #[derive(Clone, Debug, PartialEq, Facet)]
    #[repr(C)]
    pub enum DrumEvent {
        /// Transport + meters (published at meter rate while running).
        Status(DrumStatus),
        /// The drum mixer surface changed (strips / gains / mutes / meters).
        Mixer(Vec<MixerStrip>),
        /// The loaded kit changed (its pieces).
        Kit(Vec<PieceInfo>),
        /// The kit library changed (available kits).
        Library(Vec<KitInfo>),
        /// Recent MIDI activity (oldest first), for the monitor — raw events,
        /// rendered by `midicore-ui`'s panel.
        Midi(Vec<midicore_proto::MidiEvent>),
    }

    #[architect::rpc]
    pub trait DrumRig {
        /// (Re-)open the audio device and re-attach MIDI. Returns immediately;
        /// the open happens off-thread.
        fn start(&self);
        /// Close the audio device.
        fn stop(&self);
        /// Live transport + meter snapshot.
        fn status(&self) -> DrumStatus;
        /// Every kit in the library (the `.signalpreset` browser).
        fn kits(&self) -> Vec<KitInfo>;
        /// Load kit `index` from [`kits`](Self::kits) onto the sampler.
        fn load_kit(&self, index: u32);
        /// The loaded kit's pieces (engine + trigger note + preload).
        fn pieces(&self) -> Vec<PieceInfo>;
        /// Trigger a pad from the UI: note-on at `velocity`.
        fn trigger(&self, note: u32, velocity: u32);
        /// The drum mixer surface (channels + sends + buses).
        fn mixer(&self) -> Vec<MixerStrip>;
        /// Set a channel strip's gain (dB).
        fn set_channel_gain(&self, idx: u32, db: f32);
        /// Mute/unmute a channel strip.
        fn set_channel_mute(&self, idx: u32, muted: bool);
        /// Solo/unsolo a channel strip.
        fn set_channel_solo(&self, idx: u32, soloed: bool);
        /// Set a bus strip's gain (dB).
        fn set_bus_gain(&self, idx: u32, db: f32);
        /// Mute/unmute a bus strip.
        fn set_bus_mute(&self, idx: u32, muted: bool);
        /// Enumerate hardware MIDI input ports.
        fn midi_ports(&self) -> Vec<String>;
        /// Attach a hardware MIDI input port by name (empty = detach).
        fn set_midi_port(&self, name: String);
        /// Set how the attached hardware maps onto the kit (converter source).
        fn set_input_map(&self, map: InputMap);
        /// Recent MIDI events seen by the core (oldest first), for the monitor.
        fn midi_recent(&self) -> Vec<midicore_proto::MidiEvent>;

        /// Every rig change, as it happens: meters at meter rate, kit/mixer on
        /// mutation. Remotes render from this stream instead of polling.
        #[subscribe]
        fn events(&self) -> DrumEvent;
    }
}

pub use drum::{DrumEvent, DrumRig};

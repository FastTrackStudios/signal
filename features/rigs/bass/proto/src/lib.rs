//! Bass-rig wire contract — the detachable-GUI boundary.
//!
//! The rig core is 100% headless: DI bass in → NAM amp / IR chain → out on the
//! shared Signal engine, played straight from the instrument (no notes to
//! trigger) and switched from MIDI program changes / footswitch CCs. Every
//! front-end (desktop Blitz window, browser wasm, plugin editor) is a *remote*
//! that speaks the [`bass::BassRig`] service over a vox link.
//!
//! Signal-Live framing: **rigs are presets of the one engine** — "Bass" and
//! "Synth Bass" are both presets of this rig (same DI → chain → out path,
//! different chains), and a future sampled bass is just another preset
//! [`PresetKind`], not another rig.
//!
//! All types are plain `facet::Facet` data — no Dioxus, no audio backend — so
//! this crate compiles for wasm and embedded.

use facet::Facet;
use signal_proto::block::BlockType;

// ── Wire types ────────────────────────────────────────────────────────────

/// What realizes a preset — the extensibility hook that keeps "Bass",
/// "Synth Bass", and a future sampled bass presets of the SAME rig.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Facet)]
#[repr(u8)]
pub enum PresetKind {
    /// The live DI path: bass in → (drive) → NAM amp → (cab IR) → out.
    Audio,
    /// A sampled bass driven by MIDI notes (not wired yet — listed so the
    /// preset surface is already shaped for it).
    Sample,
}

impl Default for PresetKind {
    fn default() -> Self {
        PresetKind::Audio
    }
}

/// One preset in the bass library — a complete tone the rig can switch to.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct BassPreset {
    /// Display name (e.g. "Bass", "Synth Bass").
    pub name: String,
    pub kind: PresetKind,
    /// Chain loaded and ready for gapless switching (assets present).
    pub available: bool,
    /// This is the active preset.
    pub active: bool,
    /// Short chain summary for the browser row (e.g. "DI → SVT → 8×10").
    pub summary: String,
}

/// One block in the live active-preset chain.
#[derive(Clone, PartialEq, Debug, Facet)]
pub struct BassBlock {
    /// Stable id used to address the block (bypass / param edits).
    pub id: String,
    /// Block type (for coloring).
    pub block_type: BlockType,
    /// Display name.
    pub name: String,
    /// Whether the block is currently bypassed.
    pub bypassed: bool,
}

/// Live transport + meter snapshot — the high-rate payload, batched into one
/// event so a 30 Hz meter stream is one message.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct BassStatus {
    /// Audio device open and processing.
    pub running: bool,
    /// Peak input level (DI reaching the rig), linear 0..~1.
    pub input_peak: f32,
    /// Peak output level, linear 0..~1.
    pub output_peak: f32,
    /// Per-channel peaks (linear) — stereo metering.
    pub input_peak_l: f32,
    pub input_peak_r: f32,
    pub output_peak_l: f32,
    pub output_peak_r: f32,
    /// Display name of the active preset, if any.
    pub active_preset: Option<String>,
    /// Master output trim (dB).
    pub master_trim_db: f32,
    /// The stored MIDI input port filter (empty = omni).
    pub midi_port: String,
}

// ── Service ──────────────────────────────────────────────────────────────

pub mod bass {
    //! Live bass-rig control. `BassRig` → `BassRigClient` / `BassRigService`,
    //! plus the `#[subscribe]` stream sibling (`BassRigStreamService`,
    //! `BassRigStreamSource`).

    use facet::Facet;

    use super::{BassBlock, BassPreset, BassStatus};

    /// One live rig change. Every variant carries **full state** (idempotent
    /// re-application), not a diff — a late or reconnecting subscriber is
    /// correct after the next event of each kind.
    #[derive(Clone, Debug, PartialEq, Facet)]
    #[repr(C)]
    pub enum BassEvent {
        /// Transport + meters (published at meter rate while running, and
        /// once on stop).
        Status(BassStatus),
        /// The preset library changed (presets / active selection).
        Library(Vec<BassPreset>),
        /// The active preset's chain changed (blocks / bypass).
        Chain(Vec<BassBlock>),
        /// Recent MIDI activity (oldest first), for the monitor.
        Midi(Vec<midicore_proto::MidiEvent>),
    }

    #[architect::rpc]
    pub trait BassRig {
        /// (Re-)open the audio device with the persisted prefs and reload the
        /// preset library. Returns immediately; the open happens off-thread.
        fn start(&self);
        /// Close the audio device.
        fn stop(&self);
        /// Live transport + meter snapshot.
        fn status(&self) -> BassStatus;
        /// Every preset in the library (the preset browser).
        fn presets(&self) -> Vec<BassPreset>;
        /// Activate preset `index` (gapless — chains are pre-installed).
        fn select_preset(&self, index: u32);
        /// Step to the next available preset (footswitch semantics).
        fn next_preset(&self);
        /// Step to the previous available preset.
        fn prev_preset(&self);
        /// The live active-preset chain (blocks + bypass).
        fn chain(&self) -> Vec<BassBlock>;
        /// Toggle a block's bypass (by id).
        fn toggle_block_bypass(&self, id: String);
        /// Master output trim in dB (how loud the rig is for FOH).
        fn set_master_trim(&self, db: f32);
        /// Enumerate hardware MIDI input ports.
        fn midi_ports(&self) -> Vec<String>;
        /// Store the MIDI input port filter (empty = omni) and re-attach.
        fn set_midi_port(&self, name: String);
        /// Recent MIDI events seen by the core (oldest first), for the monitor.
        fn midi_recent(&self) -> Vec<midicore_proto::MidiEvent>;
        /// Re-read the styx library from disk and rebuild the live rig —
        /// the hook for external edits (text editor, LLM, git).
        fn reload_library(&self);

        /// Every rig change, as it happens: meters at meter rate,
        /// library/chain on mutation. Remotes render from this stream
        /// instead of polling.
        #[subscribe]
        fn events(&self) -> BassEvent;
    }
}

pub use bass::{BassEvent, BassRig};

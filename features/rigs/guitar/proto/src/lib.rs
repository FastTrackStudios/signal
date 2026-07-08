//! Guitar-rig wire contract — the detachable-GUI boundary.
//!
//! The rig core is 100% headless; every front-end (desktop Blitz window,
//! browser wasm, plugin editor, external MIDI controller daemon) is a *remote*
//! that speaks these services over a vox link. Desktop serves them in-process
//! (`architect::LocalServer`); the web build connects over a WebSocket; a
//! future embedded box serves the same router from a headless web server.
//!
//! Two services:
//! - [`rig::Rig`] — live rig control: transport, footswitch stacks, the
//!   active-patch FX chain, meters.
//! - [`audio::AudioSettings`] — device enumeration + persisted I/O prefs.
//!
//! All types are plain `facet::Facet` data — no Dioxus, no audio backend —
//! so this crate compiles for wasm and embedded.

use facet::Facet;
use signal_proto::block::BlockType;

// ── Wire types ────────────────────────────────────────────────────────────

/// One selectable audio device.
#[derive(Clone, PartialEq, Debug, Facet)]
pub struct AudioDevice {
    pub name: String,
    pub channels: u16,
    pub default_sample_rate: u32,
}

/// Enumerated inputs + outputs, fetched in one call.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct AudioDevices {
    pub inputs: Vec<AudioDevice>,
    pub outputs: Vec<AudioDevice>,
}

/// Audio I/O preferences. Empty-string / `0` mean "use the system/backend
/// default" (not `Option`), matching the persisted styx representation.
#[derive(Clone, PartialEq, Debug, Facet)]
pub struct AudioPrefs {
    pub input_device: String,
    pub input_channel: u32,
    pub output_device: String,
    pub sample_rate: u32,
    pub buffer_size: u32,
}

impl Default for AudioPrefs {
    fn default() -> Self {
        Self {
            input_device: String::new(),
            input_channel: 0,
            output_device: String::new(),
            sample_rate: 48_000,
            buffer_size: 256,
        }
    }
}

/// Live transport + meter snapshot — the high-rate poll payload, batched into
/// one call so a 20 Hz meter loop is one round-trip, not five.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct RigStatus {
    /// Audio device open and processing.
    pub running: bool,
    /// Peak input level (linear 0..~1).
    pub input_peak: f32,
    /// Peak output level (linear 0..~1).
    pub output_peak: f32,
    /// Display name of the active patch, if any.
    pub active_patch: Option<String>,
}

/// One footswitch stack (folder) in the performance grid — a named rotation
/// of patches plus its live cursor/active state.
#[derive(Clone, PartialEq, Debug, Facet)]
pub struct PerfStack {
    /// Folder / footswitch name (e.g. "Clean", "Lead").
    pub name: String,
    /// Display name of the patch at the current rotation cursor.
    pub current_patch: String,
    /// Cursor position within the rotation (0-based).
    pub position: u32,
    /// Number of patches in the rotation.
    pub patch_count: u32,
    /// Whether the current patch's chain is loaded (preloaded / ready).
    pub available: bool,
    /// Whether this stack holds the currently-active patch.
    pub is_active: bool,
}

/// The live performance model: the active profile's footswitch stacks + the
/// global function-switch state (FX bypass, volume boost, tempo).
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct PerformanceModel {
    pub profile_name: String,
    pub stacks: Vec<PerfStack>,
    /// Global time/FX bypass engaged.
    pub fx_bypass: bool,
    /// Volume boost engaged.
    pub boost: bool,
    /// Current tempo (BPM) — drives the tap-tempo blink.
    pub tempo_bpm: u32,
}

/// One block in the live active-patch FX chain.
#[derive(Clone, PartialEq, Debug, Facet)]
pub struct LiveBlock {
    /// Stable id used to address the block (bypass / param edits).
    pub id: String,
    /// Block type (for coloring).
    pub block_type: BlockType,
    /// Display name.
    pub name: String,
    /// Whether the block is currently bypassed.
    pub bypassed: bool,
    /// Primary dialable param name (e.g. "mix" / "depth"), if any.
    pub param_name: Option<String>,
    /// Current primary-param value + range (for the inspector knob).
    pub param_value: f32,
    pub param_min: f32,
    pub param_max: f32,
}

// ── Services ──────────────────────────────────────────────────────────────
// One `#[architect::rpc]` trait per module (the macro emits a `Service`
// token + `serve`/`layer` verbs at module scope).

pub mod rig {
    //! Live rig control. `Rig` → `RigClient` / `RigService` / `rig_serve`,
    //! plus the `#[subscribe]` stream sibling: `RigStreamClient` /
    //! `RigStreamService`, with the `RigStreamSource` backend contract.
    use facet::Facet;

    use super::{LiveBlock, PerformanceModel, RigStatus};

    /// One live rig change. Every variant carries **full state** (idempotent
    /// re-application), not a diff — a late or reconnecting subscriber is
    /// correct after the next event of each kind.
    #[derive(Clone, Debug, PartialEq, Facet)]
    #[repr(C)]
    pub enum RigEvent {
        /// Transport + meters (published at meter rate while running, and
        /// once on stop).
        Status(RigStatus),
        /// Performance model changed (patch/stack/bypass/boost).
        Perf(PerformanceModel),
        /// The active patch's FX chain changed (blocks/bypass/params).
        Chain(Vec<LiveBlock>),
    }

    #[architect::rpc]
    pub trait Rig {
        /// (Re-)open the audio device with the persisted prefs and reload the
        /// profile. Returns immediately; the open happens off-thread.
        fn start(&self);
        /// Close the audio device.
        fn stop(&self);
        /// Live transport + meter snapshot.
        fn status(&self) -> RigStatus;
        /// Current performance model (profile + footswitch stacks + state).
        fn perf(&self) -> PerformanceModel;
        /// The live active-patch FX chain (blocks + bypass + params).
        fn chain(&self) -> Vec<LiveBlock>;
        /// Press a footswitch stack (by index): activate current / rotate.
        fn press_stack(&self, index: u32);
        /// Toggle the global time/FX bypass.
        fn toggle_fx(&self);
        /// Toggle the volume boost.
        fn toggle_boost(&self);
        /// Tap tempo.
        fn tap_tempo(&self);
        /// Toggle a block's bypass (by id).
        fn toggle_block_bypass(&self, id: String);
        /// Set a block's primary param.
        fn set_block_param(&self, id: String, param: String, value: f32);

        /// Every rig change, as it happens: meters at meter rate, perf/chain
        /// on mutation. Remotes render from this stream instead of polling.
        #[subscribe]
        fn events(&self) -> RigEvent;
    }
}

pub mod audio {
    //! Audio device settings. `AudioSettings` → `AudioSettingsClient` / …
    use super::{AudioDevices, AudioPrefs};

    #[architect::rpc]
    pub trait AudioSettings {
        /// Enumerate the available input + output devices.
        fn devices(&self) -> AudioDevices;
        /// The persisted I/O preferences.
        fn prefs(&self) -> AudioPrefs;
        /// Persist edited preferences (takes effect on the next [`rig::Rig::start`]).
        fn save_prefs(&self, prefs: AudioPrefs);
    }
}

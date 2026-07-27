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
    /// Compressor gain reduction (dB, positive = reducing) — real, from the
    /// DSP's detector.
    pub comp_gr_db: f32,
    /// Per-channel peaks (linear) — stereo metering.
    pub input_peak_l: f32,
    pub input_peak_r: f32,
    pub output_peak_l: f32,
    pub output_peak_r: f32,
}

/// One keyboard binding for the remotes to interpret.
#[derive(Debug, Default, Clone, PartialEq, Facet)]
pub struct KeyBinding {
    pub keys: String,
    pub action: String,
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
    /// The preset the current patch points at.
    pub preset: String,
    /// Module names the current patch overrides (badge icons).
    pub override_modules: Vec<String>,
}

/// The live performance model: the active profile's footswitch stacks + the
/// global function-switch state (FX bypass, volume boost, tempo).
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct PerformanceModel {
    pub profile_name: String,
    pub stacks: Vec<PerfStack>,
    /// Global time/FX bypass engaged.
    pub fx_bypass: bool,
    /// Boost pedal level in dB (`0.0` = off; cycles +1 → +2 → +3 → −1).
    pub boost_db: f32,
    /// Current tempo (BPM) — drives the tap-tempo blink.
    pub tempo_bpm: u32,
    /// Setlist song names, in order.
    pub songs: Vec<SongSlot>,
    /// Index of the current song in [`songs`](Self::songs).
    pub song_index: u32,
    /// All setlist names; [`setlist_index`](Self::setlist_index) is active.
    pub setlists: Vec<String>,
    pub setlist_index: u32,
    /// The whole song library (defaults) — pick-lists for set building.
    pub library_songs: Vec<SongSlot>,
    /// Fullscreen tuner overlay — model-driven so the footswitch (hold
    /// tap-tempo) and every remote stay in sync.
    pub tuner_visible: bool,
    /// Perform-grid mode: 0 Preset (browse the pool), 1 Profile (stacks),
    /// 2 Setlist (song-adaptive: parts + stacks).
    pub perform_mode: u32,
    /// Keyboard bindings (keymap.styx) — "ctrl+1"-style keys → rig action
    /// strings, interpreted by every remote.
    pub key_bindings: Vec<KeyBinding>,
    /// The current song's section names (Intro, V1, Chorus, …).
    pub parts: Vec<String>,
    /// Index of the current section.
    pub part_index: u32,
    /// Headphone-cue module state.
    pub headphone: HeadphoneState,
    /// Master output trim (dB) — the FOH fader next to the output meter.
    pub master_trim_db: f32,
    /// Monotonic state version — bumps on every mutation, including ones
    /// (patch repoints, preset edits) that don't change the fields above,
    /// so clients can refetch derived data (patches/presets) on change.
    pub revision: u64,
}

/// One patch in the loaded profile — the preset browser's row.
#[derive(Clone, PartialEq, Debug, Facet)]
pub struct PatchInfo {
    /// Patch name (globally unique in the profile).
    pub name: String,
    /// The stack (footswitch folder) this patch belongs to, if any.
    pub stack: String,
    /// Chain preloaded and ready for gapless switching.
    pub available: bool,
    /// This is the active patch.
    pub active: bool,
    /// The preset this patch points at.
    pub preset: String,
    /// This patch is its stack's default (first in the rotation — where the
    /// footswitch lands after a reset).
    pub default_in_stack: bool,
    /// Module names this patch overrides on its preset.
    pub override_modules: Vec<String>,
}

/// One preset in the pool — a complete tone patches point at.
#[derive(Clone, PartialEq, Debug, Facet)]
pub struct PresetInfo {
    pub name: String,
    /// The active patch points at this preset.
    pub active: bool,
    /// How many patches point at it.
    pub used_by: u32,
}

/// One song slot in the active setlist — key/tempo already resolved
/// (per-set override or the song's default).
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct SongSlot {
    pub name: String,
    /// Key for this set (e.g. "G").
    pub key: String,
    /// Tempo for this set.
    pub bpm: u32,
}

/// The headphone-cue module's state. The physical headphone bus lands with
/// engine multi-out; until then volume/self-mix are staged state and
/// `main_mute` is real (kills the main output, monitoring survives on the
/// hardware direct path).
#[derive(Clone, PartialEq, Debug, Facet)]
pub struct HeadphoneState {
    /// Headphone level (0–1).
    pub volume: f32,
    /// Your own guitar's level in your ears only (0–1).
    pub self_mix: f32,
    /// Main output muted (rehearse silently; headphones keep signal).
    pub main_mute: bool,
}

impl Default for HeadphoneState {
    fn default() -> Self {
        Self { volume: 0.8, self_mix: 0.5, main_mute: false }
    }
}

/// One tuner reading. `active: false` means no usable signal (too quiet /
/// no periodicity) — the UI shows a listening state.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct TunerReading {
    /// Signal present and pitch locked.
    pub active: bool,
    /// Detected fundamental (Hz).
    pub freq_hz: f32,
    /// Nearest note name with octave, e.g. "E2", "A#3".
    pub note: String,
    /// Distance from the note in cents (−50..+50; negative = flat).
    pub cents: f32,
}

/// One controllable parameter of a live block.
#[derive(Clone, PartialEq, Debug, Facet)]
pub struct BlockParam {
    pub name: String,
    pub value: f32,
    pub min: f32,
    pub max: f32,
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
    /// The full controllable parameter set (the Control view's surface).
    pub params: Vec<BlockParam>,
    /// The block preset loaded in this slot (e.g. "JHS Morning Glory").
    pub preset: String,
    /// The preset's selectable options (NAM captures) + current selection.
    pub options: Vec<String>,
    pub option: u32,
}

// ── Services ──────────────────────────────────────────────────────────────
// One `#[architect::rpc]` trait per module (the macro emits a `Service`
// token + `serve`/`layer` verbs at module scope).

pub mod watch;

pub mod rig {
    //! Live rig control. `Rig` → `RigClient` / `RigService` / `rig_serve`,
    //! plus the `#[subscribe]` stream sibling: `RigStreamClient` /
    //! `RigStreamService`, with the `RigStreamSource` backend contract.
    use facet::Facet;

    use super::{LiveBlock, PatchInfo, PerformanceModel, PresetInfo, RigStatus, TunerReading};

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
        /// Input spectrum, ~15 Hz: dB magnitudes (−90..0) over log-spaced
        /// bins 20 Hz–20 kHz.
        Spectrum(Vec<f32>),
        /// Compressor rolling telemetry, ~15 Hz: `(input_peaks, gain_reduction)`
        /// — both 0..1, oldest → newest, a ~4-second window.
        CompWave(Vec<f32>, Vec<f32>),
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
        /// Boost pedal tap: on/off at the remembered level (default +1 dB).
        /// Drives the active chain's "Boost" gain block.
        fn toggle_boost(&self);
        /// Boost pedal hold: rotate the level — +1 → +2 → +3 → −1 dB —
        /// engaging the boost if it was off.
        fn cycle_boost(&self);
        /// Current tuner reading from the rig input (pre-amp).
        fn tuner(&self) -> TunerReading;
        /// Jump to the next song in the setlist (recalls its starting patch).
        fn next_song(&self);
        /// Jump to the previous song in the setlist.
        fn prev_song(&self);
        /// Jump straight to setlist entry `index`.
        fn select_song(&self, index: u32);
        /// Jump to section `index` of the current song.
        fn select_part(&self, index: u32);
        /// Move setlist entry `from` to position `to` (reorder).
        fn move_song(&self, from: u32, to: u32);
        /// Every patch in the loaded profile (the preset browser).
        fn patches(&self) -> Vec<PatchInfo>;
        /// Activate patch `index` directly (browser click), bypassing the
        /// footswitch stacks.
        fn select_patch(&self, index: u32);
        /// The preset pool (what patches point at).
        fn presets(&self) -> Vec<PresetInfo>;
        /// Point patch `patch` at preset `preset` — rebuilds and reloads the
        /// profile's chains (brief audio gap; an edit-time operation).
        fn set_patch_preset(&self, patch: u32, preset: u32);
        /// Set the headphone-cue module (volume + self mix, 0–1 each).
        fn set_headphone(&self, volume: f32, self_mix: f32);
        /// Mute/unmute the main output (headphone cue survives).
        fn toggle_main_mute(&self);
        /// Master output trim in dB (how loud the rig is for FOH).
        fn set_master_trim(&self, db: f32);
        /// The most recent MIDI events seen by the core (newest last),
        /// formatted for the monitor.
        fn midi_recent(&self) -> Vec<String>;
        /// Switch the active setlist (recalls its first song).
        fn select_setlist(&self, index: u32);
        /// Select a block preset's NAM option (e.g. a pedal's gain capture).
        /// Rebuilds the chains — an edit-time operation.
        fn set_block_option(&self, id: String, option: u32);
        /// Record `seconds` of the live guitar input as the calibration DI
        /// reference, then re-measure every NAM against it. Play
        /// representatively while it runs.
        fn capture_di_reference(&self, seconds: u32);
        /// Add a pool preset from a `.nam` capture (runtime import) and
        /// persist the library.
        fn add_preset(&self, name: String, nam_path: String);
        /// Add an empty footswitch stack.
        fn add_stack(&self, name: String);
        /// Add a patch pointing at `preset`, appended to `stack`'s
        /// rotation. Rebuilds the chains.
        fn add_patch(&self, name: String, stack: String, preset: String);
        /// Create a drive block preset (single option) from a `.nam`
        /// capture; add more options by editing drive-presets.styx.
        fn add_drive_preset(&self, name: String, nam_path: String);
        /// Re-read the styx library from disk and rebuild the live rig —
        /// the hook for external edits (text editor, LLM, git).
        fn reload_library(&self);
        /// Rename a pool preset (patch pointers follow).
        fn rename_preset(&self, old: String, new_name: String);
        /// Delete a pool preset — refused while any patch points at it.
        fn delete_preset(&self, name: String);
        /// Rename a patch (stack rotations follow). Rebuilds.
        fn rename_patch(&self, old: String, new_name: String);
        /// Delete a patch (removed from every stack). Rebuilds.
        fn delete_patch(&self, name: String);
        /// Rename a stack.
        fn rename_stack(&self, old: String, new_name: String);
        /// Delete a stack (its patches stay in the pool).
        fn delete_stack(&self, name: String);
        /// Add a song to the library with default key + tempo.
        fn add_song(&self, name: String, key: String, bpm: u32);
        /// Create an empty setlist.
        fn add_setlist(&self, name: String);
        /// Append a song to a setlist by index.
        fn add_setlist_entry(&self, setlist: u32, song: String);
        /// Remove an entry from a setlist by indices.
        fn remove_setlist_entry(&self, setlist: u32, entry: u32);
        /// Set the ACTIVE setlist entry's per-set overrides: empty key /
        /// zero bpm fall back to the song's defaults.
        fn set_setlist_entry(&self, entry: u32, key: String, bpm: u32);
        /// Load a custom IR wav into a reverb block (Convolution engine);
        /// auto-saves as a patch override.
        fn set_block_ir(&self, id: String, path: String);
        /// Re-point a pool preset at a different `.nam` capture (preset
        /// editing); persists and rebuilds every patch using it.
        fn set_preset_nam(&self, index: u32, nam_path: String);
        /// Manual patch output trim (dB, on top of loudness calibration).
        fn set_patch_trim(&self, patch: u32, db: f32);
        /// Toggle the fullscreen tuner overlay on every remote.
        fn toggle_tuner(&self);
        /// Perform-grid mode (0 Preset / 1 Profile / 2 Setlist), synced.
        fn set_perform_mode(&self, mode: u32);
        /// Preset mode: play pool preset `index` directly (activates its
        /// first patch, no stack rotation).
        fn play_preset(&self, index: u32);
        /// Tap tempo.
        fn tap_tempo(&self);
        /// Toggle a block's bypass (by id).
        fn toggle_block_bypass(&self, id: String);
        /// Set a block's bypass explicitly (the rotate controls need set,
        /// not toggle, semantics).
        fn set_block_bypass(&self, id: String, bypassed: bool);
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
        /// Persist edited preferences (takes effect on the next `rig::Rig::start`).
        fn save_prefs(&self, prefs: AudioPrefs);
    }
}

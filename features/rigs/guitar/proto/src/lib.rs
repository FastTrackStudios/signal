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
#[derive(Clone, PartialEq, Eq, Debug, Facet)]
pub struct AudioDevice {
    pub name: String,
    pub channels: u16,
    pub default_sample_rate: u32,
}

/// Enumerated inputs + outputs, fetched in one call.
#[derive(Clone, PartialEq, Eq, Debug, Default, Facet)]
pub struct AudioDevices {
    pub inputs: Vec<AudioDevice>,
    pub outputs: Vec<AudioDevice>,
}

/// Audio I/O preferences. Empty-string / `0` mean "use the system/backend
/// default" (not `Option`), matching the persisted styx representation.
#[derive(Clone, PartialEq, Eq, Debug, Facet)]
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

/// What the rig costs to run, as the realtime callback measures it.
///
/// This rides on [`RigStatus`] rather than a call of its own because it is
/// wanted exactly when the meters are: while playing. The meter loop is
/// already a round-trip every 50 ms, and these are nine scalars.
///
/// Two of everything, deliberately. **Peak** answers "did the player hear a
/// dropout" — one overrun is audible and must not be averaged away. **Mean**
/// answers "is this build faster than that one" — a peak moves milliseconds
/// when the scheduler preempts one block on a busy machine and says nothing
/// about the DSP. A benchmark reads the mean; a dropout hunt reads the peak
/// and [`over_budget`](Self::over_budget).
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct RigPerf {
    /// Frames in the running block (the negotiated quantum).
    pub block_frames: u32,
    /// Negotiated sample rate, Hz — with `block_frames`, the budget.
    pub sample_rate: u32,
    /// Render time of the last block, microseconds.
    pub render_us: u32,
    /// Worst render since the peak was last reset, microseconds.
    pub peak_render_us: u32,
    /// Mean render across every block since the device opened, microseconds.
    pub mean_render_us: u32,
    /// Last block's render as a fraction of its realtime budget (0..=1).
    pub load: f32,
    /// The mean block's share of the budget (0..=1) — the comparable number.
    pub mean_load: f32,
    /// Blocks that overran their deadline. Ours; every one is a dropout.
    pub over_budget: u64,
    /// Xruns the graph reported. Can sit at zero on a follower node while
    /// audio is dropping, so read `over_budget` first.
    pub xruns: u64,
    /// Blocks rendered — the sample count behind `mean_render_us`.
    pub blocks: u64,
}

impl RigPerf {
    /// One block's realtime budget in microseconds: how long the callback has
    /// before the next one is late.
    #[must_use]
    pub fn budget_us(&self) -> u32 {
        if self.sample_rate == 0 {
            return 0;
        }
        (f64::from(self.block_frames) / f64::from(self.sample_rate) * 1e6) as u32
    }

    /// Round-trip latency the buffer itself imposes, milliseconds.
    ///
    /// One block in and one block out — what the player feels as delay
    /// between the string and the speaker, before the interface's own
    /// converters add theirs.
    #[must_use]
    pub fn buffer_latency_ms(&self) -> f32 {
        if self.sample_rate == 0 {
            return 0.0;
        }
        (self.block_frames * 2) as f32 / self.sample_rate as f32 * 1000.0
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
    /// What the rig costs to run — see [`RigPerf`].
    pub perf: RigPerf,
}

/// How a patch-levelling pass is going.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct LevelProgress {
    /// Patches measured so far.
    pub done: u32,
    /// Patches in the pass.
    pub total: u32,
    /// The patch being measured, or the last one measured when finished.
    pub patch: String,
    /// Finished — `done == total`, or the pass gave up.
    pub complete: bool,
    /// What each patch was measured at and the trim it was given, in pass
    /// order: `(patch, measured LUFS, trim dB)`. Filled as it goes, so a
    /// remote can show the table building.
    pub results: Vec<PatchLevel>,
}

/// One patch's measured loudness and the trim it was given.
#[derive(Clone, PartialEq, Debug, Facet)]
pub struct PatchLevel {
    pub patch: String,
    /// Integrated loudness of the rendered patch, LUFS.
    pub lufs: f32,
    /// Output trim applied to bring it to the target, dB.
    pub trim_db: f32,
}

/// One keyboard binding for the remotes to interpret.
#[derive(Debug, Default, Clone, PartialEq, Eq, Facet)]
pub struct KeyBinding {
    pub keys: String,
    pub action: String,
}

/// One footswitch stack (folder) in the performance grid — a named rotation
/// of patches plus its live cursor/active state.
#[derive(Clone, PartialEq, Eq, Debug, Facet)]
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
    /// The current song's sections (Intro, V1, Chorus, …) and what each
    /// recalls.
    ///
    /// Shape changed here where it could not in `songs.styx`: the wire ships
    /// with both ends, so a new field costs a recompile. The stored format
    /// ships with the *player's data*, so it got an additive field instead.
    pub parts: Vec<PerfPart>,
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
#[derive(Clone, PartialEq, Eq, Debug, Facet)]
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
#[derive(Clone, PartialEq, Eq, Debug, Facet)]
pub struct PresetInfo {
    pub name: String,
    /// The active patch points at this preset.
    pub active: bool,
    /// How many patches point at it.
    pub used_by: u32,
    /// Who captured it, as the source names them. Empty for a capture the
    /// catalog has never seen — a file someone dropped in by hand.
    pub creator: String,
    /// Licence as the source states it (`t3k`, `cc-by`, …). Shown next to
    /// the preset because the terms it arrived under travel with it.
    pub license: String,
    /// The tone's page, for an attribution link.
    pub tone_url: String,
    /// What was captured: `amp`, `amp-cab`, `pedal`, …
    pub gear: String,
    /// Whether the catalog holds cover art for this preset — so a list can
    /// leave room for a picture before asking for the bytes.
    /// Fetch it with [`Rig::preset_artwork`].
    pub has_artwork: bool,
}

/// Cover art for a preset, as bytes.
///
/// Bytes rather than a path, for the same reason TONE3000 artwork travels as
/// bytes: the file is on whichever machine runs the engine, a browser remote
/// may be on a different device entirely, and a Blitz plugin editor has no
/// browser behind it to fetch anything. A UI renders it as a `data:` URI.
#[derive(Clone, PartialEq, Eq, Debug, Default, Facet)]
pub struct Artwork {
    /// Encoded image bytes exactly as the library holds them — no
    /// re-encoding, so nothing is lost or silently transcoded.
    pub bytes: Vec<u8>,
    /// `image/jpeg`, `image/png`, … for the `data:` URI's media type.
    pub mime: String,
    /// Non-empty when the art could not be read; `bytes` is then empty.
    /// A preset with no art at all is not an error — every field is simply
    /// empty.
    pub error: String,
}

/// One song slot in the active setlist — key/tempo already resolved
/// (per-set override or the song's default).
#[derive(Clone, PartialEq, Eq, Debug, Default, Facet)]
pub struct SongSlot {
    pub name: String,
    /// Key for this set (e.g. "G").
    pub key: String,
    /// Tempo for this set.
    pub bpm: u32,
}

/// The headphone-cue module's state.
///
/// The physical headphone bus lands with engine multi-out; until then
/// volume/self-mix are staged state and `main_mute` is real (kills the main
/// output, monitoring survives on the hardware direct path).
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
        Self {
            volume: 0.8,
            self_mix: 0.5,
            main_mute: false,
        }
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

/// One section of the current song, and the patch selecting it recalls.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct PerfPart {
    pub name: String,
    /// The patch this section switches to. Empty means it recalls nothing —
    /// a label, which is what every section was before.
    pub patch: String,
}

/// One controllable parameter of a live block.
#[derive(Clone, PartialEq, Debug, Facet)]
pub struct BlockParam {
    pub name: String,
    pub value: f32,
    pub min: f32,
    pub max: f32,
    /// Whether the **active patch** has moved this away from what the chain
    /// builds it as.
    ///
    /// Every knob move is recorded as an override on the patch, immediately
    /// and permanently, so without this a player cannot tell what they have
    /// changed from what came with the preset — and has no way back.
    /// [`clear_block_param`](rig::Rig::clear_block_param) is the way back.
    #[facet(default)]
    pub overridden: bool,
}

// The live node tree lives in `signal-proto`: both rigs resolve the same
// domain, so both describe it with the same words. Re-exported here because
// this crate is what the guitar UI imports.
pub use signal_proto::live_node::{LiveNode, LivePreset};

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
    /// Whether the active patch overrides anything on this block — a
    /// parameter or its bypass. The block-level summary of
    /// [`BlockParam::overridden`], for a dot on a header.
    #[facet(default)]
    pub overridden: bool,
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

    use super::{
        Artwork, LevelProgress, LiveBlock, LiveNode, PatchInfo, PerformanceModel, PresetInfo,
        RigStatus, TunerReading,
    };

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
        /// Progress of a patch-levelling pass. Levelling renders every patch
        /// offline and a NAM block is far from realtime, so this can run for a
        /// minute: without progress a player cannot tell it from a hang.
        Levelling(LevelProgress),
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
        /// Cover art for a pool preset, by name. Every field empty when the
        /// preset has none — a hand-added capture, or a tone that published
        /// no photographs. Kept off [`PresetInfo`] so listing the pool does
        /// not drag every picture across the wire.
        fn preset_artwork(&self, preset: String) -> Artwork;
        /// Measure every patch through its whole chain and trim each to a
        /// common loudness.
        ///
        /// The rig's other two loudness mechanisms work on single blocks — a
        /// capture held at unity across its drive range, an amp's measured
        /// level — and a patch's loudness is a property of the whole chain:
        /// how many gain stages stack, where the EQ sits, how hard the
        /// compressor works, how much reverb is in the mix. So this renders
        /// each patch against the DI reference and sets its trim from what
        /// came out, which is the only measurement that answers "why is the
        /// clean patch quieter than the drive".
        ///
        /// Runs off-thread; follow it on [`RigEvent::Levelling`]. Measurements
        /// are cached per chain, so a second pass is quick and an edited patch
        /// re-measures alone.
        fn level_patches(&self);
        /// The last levelling pass's progress and results (empty before one
        /// has run) — so a remote that connects mid-pass, or after it, sees it.
        fn level_progress(&self) -> LevelProgress;
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
        ///
        /// Superseded by [`select_preset`](Self::select_preset), which
        /// addresses the same choice by node and variant id rather than by
        /// block name and list position.
        fn set_block_option(&self, id: String, option: u32);
        /// The live rig as nodes — every block **and every container**, in
        /// tree order, each with the presets it can be recalled as.
        ///
        /// What [`chain`](Self::chain) cannot say: a chain is a flat list of
        /// blocks, so a Module has nowhere to appear and nothing but a drive
        /// slot can offer a preset. This is the whole tree the rig resolved.
        fn nodes(&self) -> Vec<LiveNode>;
        /// Recall a node as one of its presets — a pedal's capture, a
        /// module's combination, an amp's model.
        ///
        /// Addressed by ids on both sides: the node keeps its id when it is
        /// renamed, and the variant keeps its id when a capture is imported
        /// ahead of it in the list. Rebuilds the chains — an edit-time
        /// operation, like `set_block_option`.
        fn select_preset(&self, node: String, preset: String);
        /// Save what a node currently sounds like as a preset of it.
        ///
        /// A preset is a **diff**: what is saved is every parameter that
        /// differs from what the node resolves to on its own, so editing the
        /// node later still reaches every preset of it. The new preset
        /// appears in [`nodes`](Self::nodes) and can be recalled with
        /// [`select_preset`](Self::select_preset).
        ///
        /// Works at any level, which is the point — a block's settings, a
        /// module's combination of them.
        fn save_preset(&self, node: String, name: String);
        /// Put a different node in this slot — a different pedal on the
        /// board, not a different capture of the same one.
        ///
        /// `with` is one of the slot's
        /// [`alternatives`](signal_proto::live_node::LiveNode::alternatives).
        fn replace_node(&self, node: String, with: String);
        /// Set what a section of the **current song** recalls. An empty
        /// `patch` clears it, making the section a label again.
        fn set_part_patch(&self, part: String, patch: String);
        /// Undo the active patch's override of one parameter, returning it
        /// to what the chain builds it as.
        fn clear_block_param(&self, id: String, param: String);
        /// Undo every override the active patch has on this block — its
        /// parameters and its bypass.
        fn clear_block_overrides(&self, id: String);
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
        /// Import a downloaded capture, routed by what was captured.
        ///
        /// `gear` is the catalog's own category (`pedal`, `amp`,
        /// `amp-cab`, …) and decides the destination: a pedal becomes a
        /// drive block preset and claims a free drive slot, anything else
        /// joins the amp preset pool. `group` is the tone the capture came
        /// from, so several captures of one pedal become several options of
        /// one preset rather than several presets holding one option each.
        ///
        /// The routing lives here rather than in a GUI because it is rig
        /// policy, and every GUI is a remote.
        fn import_capture(&self, name: String, nam_path: String, gear: String, group: String);
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

#[cfg(test)]
mod tests {
    use super::RigPerf;

    /// A block's budget is its own duration: 64 frames at 48 kHz is 1.33 ms,
    /// and a callback slower than that has already made the next one late.
    #[test]
    fn a_blocks_budget_is_its_own_duration() {
        let perf = RigPerf {
            block_frames: 64,
            sample_rate: 48_000,
            ..RigPerf::default()
        };
        assert_eq!(perf.budget_us(), 1333);

        let bigger = RigPerf {
            block_frames: 512,
            sample_rate: 48_000,
            ..RigPerf::default()
        };
        assert_eq!(bigger.budget_us(), 10_666);
    }

    /// Round-trip latency is two blocks, not one: the player waits for the
    /// buffer to fill and again for it to drain.
    #[test]
    fn round_trip_latency_is_two_blocks() {
        let perf = RigPerf {
            block_frames: 64,
            sample_rate: 48_000,
            ..RigPerf::default()
        };
        assert!(
            (perf.buffer_latency_ms() - 2.667).abs() < 0.01,
            "got {}",
            perf.buffer_latency_ms()
        );
    }

    /// Before the device opens there is no rate, and dividing by it would give
    /// a plausible-looking number for a rig that is not running.
    #[test]
    fn an_unopened_rig_reports_no_budget() {
        let perf = RigPerf::default();
        assert_eq!(perf.budget_us(), 0);
        assert_eq!(perf.buffer_latency_ms(), 0.0);
    }
}

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
    /// The whole process's CPU — audio, NAM, UI, everything — as a share of
    /// the machine (0..=1), averaged over the last half second or so.
    ///
    /// Not the same question as [`load`](Self::load): that is the audio
    /// callback against its deadline, and a rig can make every deadline
    /// while the UI burns two cores. This is what Activity Monitor would
    /// say, divided by the cores.
    #[facet(default)]
    pub cpu: f32,
    /// Logical cores — so `cpu` can also be read as cores busy.
    #[facet(default)]
    pub cores: u32,
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

/// A compressor block's rolling telemetry.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct CompTrace {
    /// The compressor block this is (`LiveBlock::name`).
    pub block: String,
    /// Input peaks, 0..1, oldest → newest, a ~4-second window.
    pub input: Vec<f32>,
    /// Gain reduction, 0..1 of 30 dB, same window.
    pub gr: Vec<f32>,
    /// Current gain reduction, dB (positive = reducing).
    pub gr_db: f32,
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
    /// Active only while held (released: back to the previous patch).
    #[facet(default)]
    pub momentary: bool,
    /// Always lands on its patch — pressing again does not rotate.
    #[facet(default)]
    pub no_rotate: bool,
    /// The part that is up tunes this switch (its own rotation or mode).
    #[facet(default)]
    pub part_tuned: bool,
    /// The song that is up tunes this switch (its rotation, landing patch or
    /// mode) — the song's own switch setup, shown under the song.
    #[facet(default)]
    pub song_tuned: bool,
    /// Every patch in the switch's rotation, in order.
    #[facet(default)]
    pub patches: Vec<String>,
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
    /// The current song's profile; empty when it keeps whatever is loaded.
    #[facet(default)]
    pub song_profile: String,
    /// The part the current song starts on; empty = the profile's default.
    #[facet(default)]
    pub start_part: String,
    /// The patch the current song opens on when it has no start part.
    #[facet(default)]
    pub start_patch: String,
    /// The profile patches the current song has changed (patch, changes) —
    /// dialled in the song, kept by the song until saved back.
    #[facet(default)]
    pub song_changes: Vec<SongChange>,
    /// What footswitches 1–5 do right now: a `SWITCH_ACTIONS` key per
    /// switch (`stack`, `tap_tempo`, `parts`, …) — the song's and the part's
    /// assignments resolved.
    #[facet(default)]
    pub switch_actions: Vec<String>,
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
    /// The preset this patch points at (`Preset · Variation` for a patch
    /// that plays a preset snapshot).
    pub preset: String,
    /// The same, apart: the preset's name, and which of its variations
    /// (snapshot) the patch plays — empty for a legacy pool patch.
    pub rig_preset: String,
    pub variation: String,
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
    /// The cab IR (`.wav`) this preset's Cab block convolves, if any. Empty
    /// when `gear` is already `amp-cab` (a full rig, nothing more needed)
    /// or when an amp-only capture hasn't had one picked yet.
    #[facet(default)]
    pub cab: String,
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

/// Everything the library holds, for the browser — one fetch, re-read when
/// [`PerformanceModel::revision`] moves.
///
/// One module's pick: which module preset, and which of its snapshots.
#[derive(Clone, PartialEq, Eq, Debug, Default, Facet)]
pub struct ModulePick {
    pub module: String,
    pub preset: String,
    pub snapshot: String,
    /// The chain blocks this module owns on the live patch (what saving it
    /// from live folds in): with [`LiveBlock::overridden`], whether the
    /// module plays differently from its saved snapshot.
    #[facet(default)]
    pub blocks: Vec<String>,
}

/// A module preset (Amp, Drive, Time, …) and its snapshots, by name.
#[derive(Clone, PartialEq, Eq, Debug, Default, Facet)]
pub struct ModulePresetEntry {
    pub module: String,
    pub name: String,
    pub snapshots: Vec<String>,
    /// What refers to it (presets, other modules, patches) — why it cannot
    /// be deleted.
    #[facet(default)]
    pub used_by: Vec<String>,
    /// The same per snapshot, parallel to `snapshots`: who picks each one,
    /// joined ("" = nothing does).
    #[facet(default)]
    pub snapshot_used_by: Vec<String>,
    /// What each snapshot holds, parallel to `snapshots` — so a list can
    /// show a Time snapshot's Delay and Reverb, or an Amp's captures,
    /// without a fetch per row.
    #[facet(default)]
    pub snapshot_info: Vec<ModuleSnapshotInfo>,
}

/// What one module snapshot holds.
#[derive(Clone, PartialEq, Eq, Debug, Default, Facet)]
pub struct ModuleSnapshotInfo {
    /// The module picks it plays (a Time snapshot's Delay and Reverb).
    pub modules: Vec<ModulePick>,
    /// The block presets it puts on blocks (a Delay snapshot's DLY 1).
    pub blocks: Vec<BlockPick>,
    /// Its captures, by file name without the extension: amp, then cab,
    /// then the second amp and cab (empty ones left out).
    pub captures: Vec<String>,
    /// The macro knobs it tunes on its blocks.
    #[facet(default)]
    pub macros: Vec<String>,
}

/// One parameter a block preset sets.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct PresetParam {
    pub name: String,
    pub value: f32,
}

/// One snapshot of a preset: its module picks.
#[derive(Clone, PartialEq, Eq, Debug, Default, Facet)]
pub struct PresetSnapshotEntry {
    pub name: String,
    pub modules: Vec<ModulePick>,
    /// How many overrides it layers on top of its modules.
    pub overrides: u32,
}

/// A preset: a composition of module presets, with snapshots.
#[derive(Clone, PartialEq, Eq, Debug, Default, Facet)]
pub struct PresetEntry {
    pub name: String,
    pub snapshots: Vec<PresetSnapshotEntry>,
    /// The patches playing it — why it cannot be deleted.
    #[facet(default)]
    pub used_by: Vec<String>,
}

/// The composition libraries, and what the active patch plays from them.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct CompositionModel {
    pub modules: Vec<ModulePresetEntry>,
    pub presets: Vec<PresetEntry>,
    /// The active patch's preset and snapshot (empty when it is not composed).
    pub active_preset: String,
    pub active_snapshot: String,
    /// The active patch's effective module picks — its preset snapshot's,
    /// with its own over them.
    pub active_modules: Vec<ModulePick>,
    /// Every block preset (`blocks.styx`): a delay, a reverb, a compressor…
    /// Module snapshots point at these, so editing one changes everywhere.
    #[facet(default)]
    pub block_presets: Vec<BlockPresetEntry>,
    /// The active patch's effective block picks, one per block.
    #[facet(default)]
    pub active_blocks: Vec<BlockPick>,
}

/// A block preset: its block type's storage key (`delay`, `reverb`, …).
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct BlockPresetEntry {
    pub block_type: String,
    pub name: String,
    /// Picking it bypasses the block.
    pub bypass: bool,
    /// What refers to it (module snapshots, presets, patches) — why it
    /// cannot be deleted.
    #[facet(default)]
    pub used_by: Vec<String>,
    /// What it sets, as saved — so a list can draw and sort it (a delay's
    /// time and feedback, a reverb's algorithm and decay).
    #[facet(default)]
    pub params: Vec<PresetParam>,
    /// The macro knobs it tunes (`delay`, `space`) — empty when the macros
    /// move it their own way.
    #[facet(default)]
    pub macros: Vec<String>,
}

/// A block preset on a chain block, by the block's name (`DLY 1`, `VERB 2`).
#[derive(Clone, PartialEq, Eq, Debug, Default, Facet)]
pub struct BlockPick {
    pub block: String,
    pub preset: String,
}

/// Patches and presets are not here: they belong to the active profile and
/// already have their own lists ([`rig::Rig::patches`], [`rig::Rig::presets`]),
/// in the order their index-addressed calls expect.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct LibraryModel {
    pub profiles: Vec<ProfileEntry>,
    pub songs: Vec<SongEntry>,
    pub setlists: Vec<SetlistEntry>,
    pub drives: Vec<DriveEntry>,
}

/// One profile: a rig's worth of presets, patches and stacks.
#[derive(Clone, PartialEq, Eq, Debug, Default, Facet)]
pub struct ProfileEntry {
    pub name: String,
    /// The rig is playing this one.
    pub active: bool,
    /// Stack names, in footswitch order.
    pub stacks: Vec<String>,
    pub patches: u32,
    /// Pool preset names — the amps this profile is built on.
    pub presets: Vec<String>,
    /// Every patch, with its stack — what a song part can pick from this
    /// profile when the part is played on it.
    pub patch_list: Vec<ProfilePatch>,
    /// The patch it lands on when loaded (its default scene); empty = the
    /// first.
    pub default_patch: String,
}

/// A patch of a profile, and the stack (footswitch slot) holding it.
#[derive(Clone, PartialEq, Eq, Debug, Default, Facet)]
pub struct ProfilePatch {
    pub name: String,
    pub stack: String,
}

/// One song in the library, with its defaults.
#[derive(Clone, PartialEq, Eq, Debug, Default, Facet)]
pub struct SongEntry {
    pub name: String,
    pub key: String,
    pub bpm: u32,
    /// Section names, in order.
    pub parts: Vec<String>,
    /// Names of the setlists it appears in — why it cannot be deleted.
    pub setlists: Vec<String>,
    /// The profile it is played on; empty keeps whatever is loaded.
    pub profile: String,
    /// The part it starts on; empty = the profile's default patch.
    pub start_part: String,
}

/// One setlist and its songs as the set plays them.
#[derive(Clone, PartialEq, Eq, Debug, Default, Facet)]
pub struct SetlistEntry {
    pub name: String,
    /// The rig is playing from this set.
    pub active: bool,
    /// Entries with the set's key/tempo already resolved.
    pub songs: Vec<SongSlot>,
}

/// One drive block preset (a pedal) and its captures.
#[derive(Clone, PartialEq, Eq, Debug, Default, Facet)]
pub struct DriveEntry {
    pub name: String,
    pub options: Vec<String>,
    /// The active profile's drive slots holding it (`Drive 1`, …).
    pub slots: Vec<String>,
}

/// A profile patch the song that is up has changed.
#[derive(Clone, PartialEq, Eq, Debug, Default, Facet)]
pub struct SongChange {
    pub patch: String,
    /// How many settings it changes.
    pub count: u32,
}

/// How a stack switch is tuned (see `Rig::tune_switch`).
#[derive(Clone, PartialEq, Eq, Debug, Default, Facet)]
pub struct SwitchTuning {
    /// The stack's index.
    pub index: u32,
    /// Its rotation; empty leaves the rotation as it is.
    pub patches: Vec<String>,
    pub momentary: bool,
    pub no_rotate: bool,
    /// For the part that is up (else the song — or, with no song, the
    /// profile).
    pub part: bool,
}

/// One section of the current song, and the patch selecting it recalls.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct PerfPart {
    pub name: String,
    /// The patch this section switches to. Empty means it stays on whatever
    /// is up — which, with [`overrides`](Self::overrides), is the common
    /// case: a chorus is usually the verse's sound with one thing changed.
    pub patch: String,
    /// What this section changes on top of that patch.
    #[facet(default)]
    pub overrides: Vec<PartOverride>,
    /// The profile this part is played on, when it is not the song's.
    #[facet(default)]
    pub profile: String,
    /// The section it belongs to (consecutive parts with one section name
    /// make one section); a part nobody grouped is its own.
    #[facet(default)]
    pub section: String,
    /// How many switches the part tunes on top of the song's.
    #[facet(default)]
    pub switch_count: u32,
    /// It plays the profile's own switches (the song's tuning steps aside).
    #[facet(default)]
    pub profile_switches: bool,
}

/// One parameter a section changes.
///
/// The section's real content. Recalling a whole patch forces a separate
/// patch for every variation — a Verb-heavy chorus of an otherwise identical
/// sound becomes a second patch to build, level and maintain. This says the
/// one thing that is different.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
#[repr(C)]
pub struct PartOverride {
    /// Block name, as it appears in the chain.
    pub block: String,
    /// Parameter name; empty when `op` is `bypass`.
    pub param: String,
    /// `set` or `bypass`. For `bypass`, `value >= 0.5` means bypassed.
    pub op: String,
    pub value: f32,
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
    /// A NAM amp or pedal's Output Level (dB): the gain after the capture,
    /// stored with the gear — an amp's on the amp module snapshot, a pedal's
    /// on its drive option — so every patch playing it has it. `None` for
    /// blocks without one. Set with [`set_block_level`](rig::Rig::set_block_level).
    #[facet(default)]
    pub output_level_db: Option<f32>,
    /// A drive board slot: which capture of its pedal it plays ("Both
    /// Sides") — the pedal itself is [`preset`](Self::preset).
    #[facet(default)]
    pub detail: String,
    /// The capture file it plays, for a tooltip.
    #[facet(default)]
    pub asset: String,
    /// A drive board slot with nothing in it: it does nothing.
    #[facet(default)]
    pub empty: bool,
}

/// One knob of the macro bar, as the bar draws it — see
/// [`macros`](rig::Rig::macros).
///
/// A macro is **relative to the patch as dialled**: at [`rest`](Self::rest)
/// it changes nothing, above it gives more of what the patch already does,
/// below it less. The patch's own values stay where they are (and stay
/// editable); the macro is an offset on top of them.
#[derive(Clone, PartialEq, Debug, Facet)]
pub struct MacroKnobView {
    /// `drive`, `delay`, `width`…
    pub id: String,
    pub label: String,
    /// Accent (hex) — the label, the arc and the pointer.
    pub color: String,
    /// Knob position, 0..1.
    pub value: f32,
    /// Where the knob sits when it changes nothing — the patch as dialled.
    /// 0.5 for most; Width rests at the patch's own spread.
    pub rest: f32,
    /// Draw the arc from 12 o'clock and read out ±% (Tone's tilt).
    pub bipolar: bool,
    /// `spread` for Width: the arc opens both ways from 12 o'clock.
    pub style: String,
    /// The value as the cell prints it ("62%", "+20%", "Mono").
    pub readout: String,
    /// How the hover panel lays the children out: `row` (one line of
    /// knobs), `dual` (Delay / Reverb: a header row and one row per block,
    /// with Type and Time links) or `grouped` (a row per block, headed by
    /// the block's name). Empty when the knob has no panel.
    pub layout: String,
    /// `dual`: the five column headers.
    pub headers: Vec<String>,
    /// The cell the panel hangs under, when it is not the knob's own
    /// (Clarity's panel sits under Delay). Empty = its own cell.
    pub anchor: String,
    /// Every param the knob and its panel move, for its tune mode (and a
    /// single knob's panel) — grouped by block, in panel order.
    #[facet(default)]
    pub tune: Vec<MacroTuneView>,
    /// Something on its panel is tuned and not saved.
    #[facet(default)]
    pub tuned: bool,
    /// The preset snapshot the patch plays (`Fender · Clean`), where the
    /// bar's positions can be kept — empty when it plays none.
    #[facet(default)]
    pub snapshot: String,
    pub children: Vec<MacroChildView>,
}

/// One knob in a macro's hover panel.
#[derive(Clone, PartialEq, Debug, Facet)]
pub struct MacroChildView {
    pub id: String,
    pub label: String,
    pub color: String,
    /// Knob position, 0..1.
    pub value: f32,
    /// The position that changes nothing.
    pub rest: f32,
    /// The block it belongs to (`DLY 1`): the row of a `dual` or
    /// `grouped` panel.
    pub group: String,
    /// Has an ON/OFF pad (the drive stages).
    pub has_pad: bool,
    /// Its block is bypassed.
    pub bypassed: bool,
    /// How to print [`param`](Self::param): `db`, `db_gain`, `hz`, `ms`,
    /// `verb_s`, `div`, `pct`, `ratio`, `delay_style`, `verb_algo`,
    /// `interval`, `semitones`.
    pub fmt: String,
    /// The value it drives, live (with every macro applied).
    pub param: f32,
    /// A second value the readout needs (a reverb's algorithm, for its
    /// time in seconds).
    pub aux: f32,
    /// An absolute choice (Type, Interval) rather than an offset: how many
    /// choices. 0 for a relative knob.
    pub steps: u32,
    /// A drive stage: its slot ("Drive 1"), shown small above the pedal —
    /// the label is the pedal.
    #[facet(default)]
    pub slot: String,
    /// A line under the label: the pedal's capture ("Both Sides").
    #[facet(default)]
    pub subtitle: String,
    /// A tooltip: the capture file.
    #[facet(default)]
    pub tooltip: String,
    /// Plays nothing (an empty drive slot): drawn dimmed, not turnable.
    #[facet(default)]
    pub empty: bool,
}

/// One param a macro moves, as tune mode draws it — values in the param's
/// units.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct MacroTuneView {
    /// The knob that moves it (`space`, `delay-fb1`), the block (id and
    /// name) and the param — the address of a [`MacroTune`].
    pub knob: String,
    pub block: String,
    pub group: String,
    pub param: String,
    pub label: String,
    pub color: String,
    /// How to print it (as [`MacroChildView::fmt`]), and a reverb's
    /// algorithm for its time.
    pub fmt: String,
    pub aux: f32,
    /// Its value now, every macro applied.
    pub live: f32,
    /// The patch's own value: where the knob at rest leaves it.
    pub base: f32,
    /// Where the param lands with the knob all the way down, and up.
    pub lo: f32,
    pub hi: f32,
    /// The param's range.
    pub min: f32,
    pub max: f32,
    /// `lin`, `log`, `exp`, `s`.
    pub curve: String,
    /// Who shapes it: `module`, `block`, `seed`, `tuning`, `stage`, or
    /// empty (the engine's own relative response).
    pub source: String,
    /// A drive stage: where on the knob's upper half (0..1) it comes in;
    /// −1 otherwise.
    pub enter: f32,
    /// Draw the range by ratio (times, frequencies).
    pub log: bool,
    /// Kept off this param (the `off` flag).
    pub off: bool,
    /// Who shapes it below the tuning: `module`, `block`, `seed`, `stage`,
    /// or empty (the engine's own) — where a reset goes back to.
    pub inherited: String,
    /// The tuning sets its bottom / its top / anything.
    pub min_set: bool,
    pub max_set: bool,
    pub edited: bool,
}

/// One tune-mode edit — [`rig::Rig::tune_macro`]. `op`: `min`, `max`,
/// `enter` (`value`), `curve` (`text`), `off` (`value` ≥ 0.5 = off), or back
/// to what the presets say: `reset_min`, `reset_max`, `reset_curve`,
/// `reset_off`, `reset_enter`, `reset`.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct MacroTune {
    pub knob: String,
    pub block: String,
    pub param: String,
    pub op: String,
    pub value: f32,
    pub text: String,
}

/// Save what is tuned on a bar knob's panel — [`rig::Rig::save_macro_tune`].
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct MacroSave {
    pub knob: String,
    /// `block` (each block's block preset) or `module` (the module snapshot
    /// that owns the block).
    pub scope: String,
    /// A new block preset / module snapshot to create for the blocks that
    /// have none in that scope. Empty = save only where there is one.
    pub name: String,
}

/// What a macro call did — shown in the panel's header.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct MacroResult {
    pub ok: bool,
    /// "Saved to Ambient Dotted", or why not.
    pub message: String,
    /// What would make it work: `new_block_preset` / `new_module_snapshot`
    /// (ask for a name and save again with it). Empty otherwise.
    pub offer: String,
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
        Artwork, CompTrace, CompositionModel, LevelProgress, LibraryModel, LiveBlock, LiveNode, MacroKnobView, MacroResult,
        MacroSave, MacroTune,
        PartOverride, PatchInfo, PerformanceModel, PresetInfo, RigStatus, SongChange, SwitchTuning, TunerReading,
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
        /// One compressor block's rolling telemetry, ~15 Hz — sent per
        /// compressor, each from its own meter channel, so every compressor
        /// panel draws its own block.
        CompWave(CompTrace),
        /// Progress of a patch-levelling pass. Levelling renders every patch
        /// offline and a NAM block is far from realtime, so this can run for a
        /// minute: without progress a player cannot tell it from a hang.
        Levelling(LevelProgress),
        /// The macro bar changed — a knob moved, the patch switched, or a
        /// param the bar shows was edited.
        Macros(Vec<MacroKnobView>),
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
        /// Press a footswitch stack (by index): activate current / rotate —
        /// or, on a momentary switch, activate it until
        /// [`release_stack`](Self::release_stack).
        fn press_stack(&self, index: u32);
        /// Release a footswitch stack: a momentary switch goes back to the
        /// patch that was playing before its press. No-op otherwise.
        fn release_stack(&self, index: u32);
        /// Set how switch `index` behaves for the song that is up (or the
        /// profile, with no song): momentary, and/or no rotation.
        fn set_stack_mode(&self, index: u32, momentary: bool, no_rotate: bool);
        /// Tune switch (stack) `index`: its rotation (empty = leave it) and
        /// mode — for the part that is up when `part`, else the song (or the
        /// profile, with no song).
        fn tune_switch(&self, tuning: SwitchTuning);
        /// Drop the part's (`part`) or the song's tuning of switch `index`.
        fn reset_switch(&self, index: u32, part: bool);
        /// Give footswitch `switch` (0-based, 0–4) a job — a `SWITCH_ACTIONS`
        /// key, empty for its usual one — for the part (`part`) or the song.
        fn set_switch_action(&self, switch: u32, action: String, part: bool);
        /// Footswitch `switch` (0-based) tapped / held, as the pedal does it
        /// — whatever job it has right now.
        fn tap_switch(&self, switch: u32);
        fn hold_switch(&self, switch: u32);
        /// Step through the song: parts (`sections` false) or sections,
        /// forward (`dir` > 0) or back.
        fn step_part(&self, dir: i32, sections: bool);
        /// Put `part` in section `section` (empty = its own).
        fn set_part_section(&self, part: String, section: String);
        /// Whether `part` plays the profile's own switches.
        fn set_part_profile_switches(&self, part: String, on: bool);
        /// Save the song's changes to profile patch `patch` back into the
        /// profile (its new default) and clear them from the song. Returns
        /// what happened.
        fn save_song_changes(&self, patch: String) -> String;
        /// Drop the song's changes to `patch`: it plays as the profile has it.
        fn discard_song_changes(&self, patch: String) -> String;
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
        /// Load a second amp (Amp R) into the patch: Amp L → Cab L and
        /// Amp R → Cab R run in parallel and are blended. Independently
        /// bypassable. `preset` indexes the same pool `set_patch_preset` does.
        fn set_patch_preset2(&self, patch: u32, preset: u32);
        /// Unload Amp R — the slot goes back to an empty, bypassed passthrough.
        fn clear_patch_preset2(&self, patch: u32);
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
        /// Apply every library file edited since the rig loaded it, now —
        /// without waiting for the file watcher. Files that differ from
        /// what the rig holds are re-read and applied live (gapless chain
        /// rebuilds; the playing patch, song and part kept); a file that
        /// does not parse changes nothing. One line per file applied, or a
        /// line saying nothing had changed.
        fn reload_config(&self) -> String;
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
        /// Name a new section on the current song, appended at the end.
        ///
        /// Sections are the song's structure, so they are ordered and named
        /// by the player rather than derived from anything — an "Instrumental
        /// 2" exists because the song has one.
        fn add_part(&self, name: String);
        /// Rename a section, keeping what it recalls and changes.
        ///
        /// Recalls are keyed by NAME, so a rename that did not carry them
        /// across would silently empty the section — which looks like the
        /// rename worked and the section was always blank.
        fn rename_part(&self, old: String, new_name: String);
        /// Remove a section and whatever it recalled.
        fn remove_part(&self, name: String);
        /// Move a section to a new position, for arranging a song.
        fn move_part(&self, from: u32, to: u32);
        /// Set what a section changes on top of its patch.
        ///
        /// Replaces the whole list for that section, so a remote sends the
        /// set it wants rather than diffing — an override list is short and
        /// a partial-update protocol for it would be more moving parts than
        /// the thing it edits.
        fn set_part_overrides(&self, part: String, overrides: Vec<PartOverride>);
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
        /// Point a pool preset's Cab block at an IR wav (empty clears it,
        /// making the Cab block a passthrough again — for a preset whose
        /// `.nam` is already a full rig). Persists and rebuilds every patch
        /// using it.
        fn set_preset_cab(&self, index: u32, ir_path: String);
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
        /// Set a NAM amp or pedal's Output Level (dB). `commit = false` moves
        /// the live block only (a drag in progress); `commit = true` saves it
        /// with the gear (amp module snapshot / drive option) and rebuilds,
        /// so every patch playing that amp or pedal has it.
        fn set_block_level(&self, id: String, level_db: f32, commit: bool);

        /// Every profile, song, setlist and drive preset — the browser's
        /// view of the library.
        fn library(&self) -> LibraryModel;
        /// The module-preset and preset libraries, and the active patch's
        /// picks from them.
        fn compositions(&self) -> CompositionModel;
        /// Play `module`'s preset `preset` (at `snapshot`; empty = its
        /// first) on the active patch — saved as the patch's own pick, over
        /// whatever its preset snapshot chose.
        fn choose_module(&self, module: String, preset: String, snapshot: String);
        /// Put block preset `preset` on the active patch's block `block`
        /// (`DLY 1`, `VERB 2`, …) — the patch's own pick, saved and rebuilt.
        fn choose_block(&self, block: String, preset: String);
        /// Step the active patch's `module` pick through its preset's
        /// snapshots (`delta` −1 / +1, wrapping). With no pick yet, takes
        /// the module's first preset.
        fn step_module(&self, module: String, delta: i32);
        /// Point the active patch at a preset snapshot.
        fn choose_preset(&self, preset: String, snapshot: String);
        /// Step the active patch through its preset's snapshots.
        fn step_preset_snapshot(&self, delta: i32);
        /// Play a different profile. Rebuilds the rig from it (an audio gap,
        /// like any rebuild) and remembers it across restarts.
        fn select_profile(&self, name: String);
        /// Create a profile. With `from` naming a profile, a copy of it;
        /// empty, a starter holding the active profile's presets and drive
        /// slots with one stack and one patch, so it plays from the start.
        fn add_profile(&self, name: String, from: String);
        /// Rename a profile (its file follows).
        fn rename_profile(&self, old: String, new_name: String);
        /// Delete a profile — refused for the one playing, and the last.
        fn delete_profile(&self, name: String);
        /// Edit a song's defaults; renaming carries its setlist entries.
        fn edit_song(&self, old: String, name: String, key: String, bpm: u32);
        /// Delete a song — refused while any setlist holds it.
        fn delete_song(&self, name: String);
        /// Rename setlist `index`.
        fn rename_setlist(&self, index: u32, new_name: String);
        /// Copy setlist `index` as `new_name` (next week's set from this one).
        fn duplicate_setlist(&self, index: u32, new_name: String);
        /// Delete setlist `index` — refused for the last one.
        fn delete_setlist(&self, index: u32);
        /// The profile a song is played on (empty: keep whatever is loaded).
        fn set_song_profile(&self, song: String, profile: String);
        /// The part a song starts on (empty: the profile's default patch).
        fn set_song_start_part(&self, song: String, part: String);
        /// The profile a part of the **current song** is played on (empty:
        /// the song's).
        fn set_part_profile(&self, part: String, profile: String);
        /// The patch a profile lands on when loaded — its default scene
        /// (empty: the first patch).
        fn set_profile_default(&self, profile: String, patch: String);

        // ── Library management: module, block and composed presets ──
        // Each saves its library file and rebuilds, like the edits above. A
        // delete of something still referenced is refused (the entries'
        // `used_by` says by what, so a remote can say so before asking).

        /// Save what `module` plays on the live patch as snapshot `snapshot`
        /// of its preset `preset` — creating either when new, replacing the
        /// snapshot when it exists (which is "update from live"). The
        /// patch's own edits on the module's blocks move into the snapshot
        /// and the patch plays it.
        fn save_module_snapshot(&self, module: String, preset: String, snapshot: String);
        /// Drop the live patch's own edits on `module`'s blocks: back to the
        /// module snapshot as saved.
        fn revert_module(&self, module: String);
        /// Rename a module preset; presets, modules and patches follow.
        fn rename_module_preset(&self, module: String, old: String, new_name: String);
        /// Copy a module preset, snapshots and all, as `new_name`.
        fn duplicate_module_preset(&self, module: String, name: String, new_name: String);
        /// Delete a module preset — refused while anything refers to it.
        fn delete_module_preset(&self, module: String, name: String);
        /// Rename one snapshot of a module preset; references follow.
        fn rename_module_snapshot(&self, module: String, preset: String, old: String, new_name: String);
        /// Delete one snapshot — refused for the last one, or while a
        /// preset or patch names it.
        fn delete_module_snapshot(&self, module: String, preset: String, snapshot: String);
        /// Save the live block `block` (`DLY 1`) as block preset `name` of
        /// its type — new, or replacing that preset's settings ("update").
        /// The patch's own edits on the block go; it plays the preset.
        fn save_block_preset(&self, block: String, name: String);
        /// Rename a block preset; module snapshots, presets and patches
        /// follow.
        fn rename_block_preset(&self, old: String, new_name: String);
        /// Copy a block preset as `new_name`.
        fn duplicate_block_preset(&self, name: String, new_name: String);
        /// Delete a block preset — refused while anything refers to it.
        fn delete_block_preset(&self, name: String);
        /// Rename a preset (composition); patches follow.
        fn rename_rig_preset(&self, old: String, new_name: String);
        /// Copy a preset, snapshots and all, as `new_name`.
        fn duplicate_rig_preset(&self, name: String, new_name: String);
        /// Delete a preset — refused while a patch plays it.
        fn delete_rig_preset(&self, name: String);
        /// Copy a song — sections, recalls, tuning — as `new_name`. Its own
        /// patches are copied under names of their own.
        fn duplicate_song(&self, name: String, new_name: String);
        /// Move an entry within setlist `setlist` (any set, not only the one
        /// playing).
        fn move_setlist_entry(&self, setlist: u32, from: u32, to: u32);
        /// Move a setlist in the list of sets.
        fn move_setlist(&self, from: u32, to: u32);

        /// The active patch's macro bar, values included.
        fn macros(&self) -> Vec<MacroKnobView>;
        /// Move macro `id` (a bar knob or one in its panel) to `value`
        /// (0..1). A bar knob sets its panel's knobs; they set the patch's
        /// params as offsets from the patch as dialled (never recorded as
        /// patch edits). Kept with the patch.
        fn set_macro(&self, id: String, value: f32) -> MacroResult;
        /// Double-click in play: a bar knob back to rest, a panel knob back
        /// to where its bar knob puts it.
        fn reset_macro(&self, id: String) -> MacroResult;
        /// A drive stage's ON/OFF pad: force the stage on or off until its
        /// bar knob next moves.
        fn set_macro_pad(&self, id: String, on: bool) -> MacroResult;
        /// Tune one param's response (where it lands at the knob's ends,
        /// the curve, off, a drive stage's entry) — live, until saved or
        /// discarded.
        fn tune_macro(&self, tune: MacroTune) -> MacroResult;
        /// Save what is tuned on a bar knob's panel into the presets. With
        /// no home in that scope for a block, the result offers a new block
        /// preset or module snapshot; saving again with a name creates it.
        fn save_macro_tune(&self, save: MacroSave) -> MacroResult;
        /// Clear what `scope` (`block` / `module`) says about bar knob
        /// `knob`'s params, so the next layer down plays.
        fn reset_macro_scope(&self, knob: String, scope: String) -> MacroResult;
        /// Throw away what is tuned on `knob`'s panel.
        fn discard_macro_tune(&self, knob: String) -> MacroResult;
        /// Keep the bar's knob positions: `patch` (the active patch, which
        /// always can) or `snapshot` (its preset snapshot — every patch
        /// playing it starts there).
        fn save_macro_positions(&self, scope: String) -> MacroResult;

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

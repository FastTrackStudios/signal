//! The performance model every rig shares: presets and their variations
//! (snapshots), the modules a preset composes, profiles of footswitch
//! stacks, and the songs and setlists that play them.
//!
//! Born as the guitar rig's; every rig speaks it now. A drum kit is a
//! preset and its mixes its variations; each kit piece is a module, its
//! instruments the module's presets. Fields one rig has no use for stay
//! empty (a drum kit has no captures, no drive slots).

use facet::Facet;

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
    /// What the current patch plays: its Core (`preset · snapshot`), and
    /// its Time module when it picks one — the amp, for a patch on no Core.
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
    /// What kind of sound it is: Clean, Crunch, Drive, Lead or Ambient —
    /// its own tag, else its stack's or variation's (empty: none).
    #[facet(default)]
    pub role: String,
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
    /// The patch it starts on — the setlist's badge. Empty: it keeps the
    /// profile's default.
    pub start: String,
    /// Its colour (`#rrggbb`); empty: one picked from its name.
    #[facet(default)]
    pub colour: String,
}

/// One module's pick: which module preset, and which of its snapshots.
#[derive(Clone, PartialEq, Eq, Debug, Default, Facet)]
pub struct ModulePick {
    pub module: String,
    pub preset: String,
    pub snapshot: String,
    /// The chain blocks this module owns on the live patch (what saving it
    /// from live folds in): with the live block's `overridden`, whether the
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
    /// The NAM models it loads, by role: `amp` and `cab` (its captures),
    /// `drive` (a slot's pedal: the capture, the pedal and its option).
    #[facet(default)]
    pub models: Vec<CaptureModel>,
    /// The macro knobs it tunes on its blocks.
    #[facet(default)]
    pub macros: Vec<String>,
    /// A Core snapshot frozen into NAM captures (`signal rig freeze`): it
    /// can play them in place of its settings.
    #[facet(default)]
    pub frozen_available: bool,
    /// It plays its frozen captures now.
    #[facet(default)]
    pub frozen: bool,
}

/// A NAM model a module snapshot loads.
#[derive(Clone, PartialEq, Eq, Debug, Default, Facet)]
pub struct CaptureModel {
    /// `amp`, `cab` or `drive`.
    pub role: String,
    /// The capture, by file stem (a drive: its option's, else the pedal).
    pub name: String,
    /// A drive's pedal and the option it plays; empty otherwise.
    #[facet(default)]
    pub pedal: String,
    #[facet(default)]
    pub option: String,
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
    /// The picks put over it (module kind or `block:<block>`), as they
    /// play now.
    #[facet(default)]
    pub picks: Vec<PartPick>,
    /// Its block presets, by block.
    #[facet(default)]
    pub blocks: Vec<BlockPick>,
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

/// Everything the library holds, for the browser — one fetch, re-read when
/// the model's `revision` moves.
///
/// Patches and presets are not here: they belong to the active profile and
/// already have their own lists (the rig's `patches` and `presets`),
/// in the order their index-addressed calls expect.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct LibraryModel {
    pub profiles: Vec<ProfileEntry>,
    pub songs: Vec<SongEntry>,
    pub setlists: Vec<SetlistEntry>,
    pub drives: Vec<DriveEntry>,
    /// Song collections ("Church"…): a song can be in any number.
    #[facet(default)]
    pub collections: Vec<CollectionEntry>,
}

/// A section's own pick: `kind` is a module kind (`Amp`, `Time`…) or
/// `block:<block name>`; the preset, and the snapshot for a module.
#[derive(Clone, PartialEq, Eq, Debug, Default, Facet)]
pub struct PartPick {
    pub kind: String,
    pub preset: String,
    #[facet(default)]
    pub snapshot: String,
}

/// A named group of songs, for finding them faster.
#[derive(Clone, PartialEq, Eq, Debug, Default, Facet)]
pub struct CollectionEntry {
    pub name: String,
    /// `#rrggbb`.
    pub colour: String,
    pub songs: Vec<String>,
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
    /// The song whose own patch it is (merged into the profile while the
    /// song plays); empty for the profile's own.
    #[facet(default)]
    pub song: String,
}

/// One song in the library, with its defaults.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct SongEntry {
    pub name: String,
    pub key: String,
    pub bpm: u32,
    /// Section names, in order.
    pub parts: Vec<String>,
    /// Its parts as the setlist shows them (what each plays, its section,
    /// its changes) — for a song that is not up.
    #[facet(default)]
    pub part_list: Vec<PerfPart>,
    /// Names of the setlists it appears in — why it cannot be deleted.
    pub setlists: Vec<String>,
    /// The profile it is played on; empty keeps whatever is loaded.
    pub profile: String,
    /// The part it starts on; empty = the profile's default patch.
    pub start_part: String,
    /// Its colour (`#rrggbb`); empty: one picked from its name.
    #[facet(default)]
    pub colour: String,
    /// Who it's by; empty when unknown.
    #[facet(default)]
    pub artist: String,
    /// Its genre ("Worship", "Gospel", …); empty when unset.
    #[facet(default)]
    pub genre: String,
}

/// One setlist and its songs as the set plays them.
#[derive(Clone, PartialEq, Eq, Debug, Default, Facet)]
pub struct SetlistEntry {
    pub name: String,
    /// The rig is playing from this set.
    pub active: bool,
    /// Entries with the set's key/tempo already resolved.
    pub songs: Vec<SongSlot>,
    /// The day it is played (`YYYY-MM-DD`); empty when it has none.
    #[facet(default)]
    pub date: String,
    /// The recurring event it is for ("HSM"); empty for a one-off.
    #[facet(default)]
    pub event: String,
    /// A title of its own ("Worship Night"); empty: named by event and day.
    #[facet(default)]
    pub title: String,
    /// The profile every song plays on unless it has its own; empty keeps
    /// whatever is loaded.
    #[facet(default)]
    pub profile: String,
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

/// One section of the current song, and the patch selecting it recalls.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct PerfPart {
    pub name: String,
    /// The patch this section switches to. Empty means it stays on whatever
    /// is up — which, with [`overrides`](Self::overrides), is the common
    /// case: a chorus is usually the verse's sound with one thing changed.
    pub patch: String,
    /// It plays this whole stack (its first patch, then the switch steps
    /// it) — empty when it plays a patch.
    #[facet(default)]
    pub stack: String,
    /// It plays this preset's variation (`Preset · Variation`) — empty when
    /// it plays a patch.
    #[facet(default)]
    pub preset: String,
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
    /// The part it repeats (plays and edits that part's sound); empty for
    /// its own.
    #[facet(default)]
    pub repeat_of: String,
    /// The section's own picks over its patch: module presets and block
    /// presets it plays (their overrides are in `overrides`).
    #[facet(default)]
    pub picks: Vec<PartPick>,
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

/// Whether a pick took — what the browser shows on the row picked: Loaded,
/// or Didn't load and why. A refusal the rig used to only log (no such
/// preset, no patch playing, a block that has gone) comes back as `message`.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct Applied {
    pub ok: bool,
    /// Why not, when it did not take. Empty when it did.
    pub message: String,
}

impl Applied {
    /// It took.
    #[must_use]
    pub fn done() -> Self {
        Self {
            ok: true,
            message: String::new(),
        }
    }

    /// It did not, and why.
    #[must_use]
    pub fn refused(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            message: message.into(),
        }
    }
}

/// What every rig's performance surface shows: the mode, the profile's
/// stacks, the set and the song that is up. Carried by [`PerformEvent`] on
/// every change.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct PerformState {
    /// 0 Preset (browse the presets), 1 Profile (its stacks), 2 Setlist
    /// (the song's parts and the stacks).
    pub mode: u32,
    pub profile_name: String,
    pub stacks: Vec<PerfStack>,
    /// The patch playing, if any.
    pub active_patch: String,
    /// The setlist that is up, in order, and the song that is up in it.
    pub songs: Vec<SongSlot>,
    pub song_index: u32,
    /// Every setlist's name; `setlist_index` is up.
    pub setlists: Vec<String>,
    pub setlist_index: u32,
    /// The whole song library — the pick-lists for building a set.
    pub library_songs: Vec<SongSlot>,
    /// The current song's sections, and the one that is up.
    pub parts: Vec<PerfPart>,
    pub part_index: u32,
    /// The current song's profile (empty: whatever is loaded), the part it
    /// starts on, and the patch it opens on without one.
    pub song_profile: String,
    pub start_part: String,
    pub start_patch: String,
    /// The profile patches the current song has changed.
    pub song_changes: Vec<SongChange>,
    pub tempo_bpm: u32,
    /// How many sound choices undo can take back.
    pub undo_depth: u32,
    /// Bumps on every change, including ones the fields above do not show
    /// (a preset edited) — refetch the lists when it moves.
    pub revision: u64,
}

pub mod service {
    //! `RigPerform` → `RigPerformClient` / `RigPerformService`, and the
    //! stream twin `RigPerformStreamClient` / `RigPerformStreamService` with
    //! its `RigPerformStreamSource` backend contract.
    //!
    //! Mounted once per rig under the rig's scope
    //! (`merge_router_scoped(rig.slug(), …)`); a client picks its rig with
    //! `architect::scope_client!(client, rig.slug())`.

    use facet::Facet;

    use super::{Applied, CompositionModel, LibraryModel, PatchInfo, PerformState};

    /// One performance change: the whole state, so a late subscriber is
    /// right after the next one.
    #[derive(Clone, Debug, PartialEq, Facet)]
    #[repr(C)]
    pub enum PerformEvent {
        State(PerformState),
    }

    #[architect::rpc]
    pub trait RigPerform {
        /// The performance state now.
        fn state(&self) -> PerformState;
        /// 0 Preset / 1 Profile / 2 Setlist — what the sidebar shows and the
        /// switches play.
        fn set_perform_mode(&self, mode: u32);

        // ── Presets and variations ──
        /// Every patch in the loaded profile.
        fn patches(&self) -> Vec<PatchInfo>;
        /// Play patch `index` directly.
        fn select_patch(&self, index: u32) -> Applied;
        /// The presets and their variations, the modules they compose, and
        /// what the patch playing picks from them.
        fn compositions(&self) -> CompositionModel;
        /// Play a preset's variation.
        fn choose_preset(&self, preset: String, variation: String);
        /// Save what plays now as a preset's variation.
        fn save_variation(&self, preset: String, variation: String);
        fn rename_preset(&self, old: String, new_name: String);
        fn duplicate_preset(&self, name: String, new_name: String);
        /// Refused while a patch plays it.
        fn delete_preset(&self, name: String);

        // ── Profiles and their stacks ──
        /// Every profile, song and setlist.
        fn library(&self) -> LibraryModel;
        fn select_profile(&self, name: String) -> Applied;
        /// A new profile: a copy of `from`, or (empty) a starter.
        fn add_profile(&self, name: String, from: String);
        fn rename_profile(&self, old: String, new_name: String);
        /// Refused for the one playing, and the last.
        fn delete_profile(&self, name: String);
        /// Press stack `index`: play it, or step its rotation.
        fn press_stack(&self, index: u32);
        /// Play stack `index`'s patch at `position` in its rotation.
        fn play_stack_patch(&self, index: u32, position: u32) -> Applied;
        /// Put `patch` in stack `stack`'s rotation, or take it out.
        fn set_stack_patch(&self, stack: String, patch: String, on: bool);
        fn move_stack_patch(&self, stack: String, from: u32, to: u32);
        fn move_stack(&self, from: u32, to: u32);
        fn rename_stack(&self, old: String, new_name: String);
        fn rename_patch(&self, old: String, new_name: String);

        // ── Songs and their parts ──
        fn select_song(&self, index: u32) -> Applied;
        fn select_part(&self, index: u32) -> Applied;
        fn add_part(&self, name: String);
        fn rename_part(&self, old: String, new_name: String);
        fn remove_part(&self, name: String);
        fn move_part(&self, from: u32, to: u32);
        /// What a part of the current song plays (empty: a label).
        fn set_part_patch(&self, part: String, patch: String);
        /// The section a part belongs to (empty: its own).
        fn set_part_section(&self, part: String, section: String);
        /// The profile a part plays on (empty: the song's).
        fn set_part_profile(&self, part: String, profile: String);
        /// A part edit on any song: `op` `add`, `rename`, `remove`, `move`,
        /// `section` or `patch`.
        fn edit_song_part(&self, song: String, op: String, part: String, value: String);

        /// A new song in the library.
        fn add_song(&self, name: String, key: String, bpm: u32);
        /// The profile a song plays on (empty: whatever is loaded).
        fn set_song_profile(&self, song: String, profile: String);
        /// A song's colour (`#rrggbb`; empty: one from its name).
        fn set_song_colour(&self, song: String, colour: String);

        // ── Setlists ──
        /// Play from setlist `index`.
        fn select_setlist(&self, index: u32) -> Applied;
        fn add_setlist(&self, name: String);
        fn rename_setlist(&self, index: u32, new_name: String);
        /// Copy setlist `index` as `new_name`.
        fn duplicate_setlist(&self, index: u32, new_name: String);
        /// A setlist's event, day (`YYYY-MM-DD`) and title.
        fn set_setlist_details(&self, index: u32, event: String, date: String, title: String);
        /// The profile a setlist's songs play on unless they have their own.
        fn set_setlist_profile(&self, index: u32, profile: String);
        /// Append a song to setlist `setlist`.
        fn add_setlist_entry(&self, setlist: u32, song: String);
        /// An entry of the set that is up: its key and tempo for this set.
        fn set_setlist_entry(&self, entry: u32, key: String, bpm: u32);
        /// The patch an entry of the set that is up starts on.
        fn set_setlist_entry_start(&self, entry: u32, patch: String);
        fn move_setlist_entry(&self, setlist: u32, from: u32, to: u32);
        fn remove_setlist_entry(&self, setlist: u32, entry: u32);
        /// Refused for the last one.
        fn delete_setlist(&self, index: u32);

        /// Every change, as it happens.
        #[subscribe]
        fn events(&self) -> PerformEvent;
    }
}

pub use service::{PerformEvent, RigPerform};

/// Mount a rig's [`RigPerform`] (and its stream) on a router under the
/// rig's scope — the one way every router assembly does it:
/// `signal_rigs_proto::mount_perform!(router, Rig::Guitar, backend.clone())`.
#[macro_export]
macro_rules! mount_perform {
    ($router:expr, $rig:expr, $backend:expr) => {{
        use ::architect::Layer as _;
        let rig: $crate::Rig = $rig;
        $router.merge_router_scoped(
            rig.slug(),
            ::architect::layers![
                $crate::perform::service::Service,
                $crate::perform::service::StreamService
            ]
            .provide($backend),
        )
    }};
}

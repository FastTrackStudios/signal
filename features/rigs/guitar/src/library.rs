//! The runtime rig library — everything the rig plays from, as plain styx
//! text files in one directory.
//!
//! Portable, git-trackable, and directly editable: an LLM (or a human in a
//! text editor) can create presets, add patches, write patch-level overrides,
//! rename things — then hit reload.
//!
//! ```text
//! <config>/signal/rig/          (override: SIGNAL_RIG_DIR)
//!   profiles/<name>.styx  ProfileDef, one per file — presets pool, patches
//!                       (+overrides), stacks, drive-slot assignments.
//!                       `last-state.styx` names the active one
//!   drive-presets.styx  DrivePresetLib — block presets (NAM option sets)
//!   songs.styx          SongLib — the song library (key/bpm defaults)
//!   setlists.styx       SetlistLib — dated sets with per-entry overrides
//! ```
//!
//! First run bootstraps the files from the in-repo default config
//! (`features/rigs/guitar/default-config/` — a snapshot of the worship
//! rig, embedded at compile time), including the NAM model files it
//! references, so the directory is always a complete, editable snapshot
//! and a fresh engine makes sound out of the box. NAM paths in the
//! defaults are relative (`models/<file>.nam`) and resolve against the
//! rig directory at load; absolute paths pass through untouched.
//!
//! # Running without leaving a mark
//!
//! Two environment flags, for measuring and testing the real rig rather than
//! a stand-in of it:
//!
//! | flag | effect |
//! |---|---|
//! | `SIGNAL_RIG_EPHEMERAL=1` | every save is a no-op — the rig plays the real library and forgets everything, so a test run cannot move the player's position or edit their profile |
//! | `SIGNAL_RIG_SILENT=1` | the master is muted (−96 dB after the chain), so every block still processes and nothing is heard |
//! | `SIGNAL_RIG_DESIGN=1` | no audio device, no MIDI, no DSP — the UI over a synthesised rig, so several can run at once. Implies ephemeral |
//! | `SIGNAL_RIG_DIR=<path>` | a different library entirely — isolation, but a different rig |

use std::path::{Path, PathBuf};

use facet::Facet;
use signal_rig_host::store::{StyxDir, signal_config_dir};

use crate::config_watch::{self, Read};
use crate::instrument::Instrument;
use crate::profiles::{
    DrivePresetDef, KeyBindingDef, MidiMapDef, ProfileDef, SetlistDef, SongDef, default_keymap,
    default_midi_map, default_setlists, drive_presets, song_library, worship_def,
};

/// The library directory a host chose for this process ([`use_rig_dir`]).
static CHOSEN_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Play from `dir` for the rest of this process, instead of the player's
/// library — a host that keeps its own copy (Session, whose scenes are a
/// mix's, not the gig rig's). Seeded from the shipped defaults like any
/// other library the first time it is read. Set once, before the library
/// is first read; `false` if it was already set.
///
/// This moves the process's own library ([`RigHome::process_default`]),
/// so there can be only one. A host that wants its own library — or
/// several, a guitar's and a bass's side by side — gives each rig a
/// [`RigHome`] instead ([`RigHome::new`], `GuitarRigBackend::with_home`)
/// and leaves the process's alone.
pub fn use_rig_dir(dir: PathBuf) -> bool {
    CHOSEN_DIR.set(dir).is_ok()
}

/// The library directory (a host's [`use_rig_dir`], else `SIGNAL_RIG_DIR`,
/// else the instrument's own under the config root).
#[must_use]
pub fn rig_dir() -> PathBuf {
    if let Some(dir) = CHOSEN_DIR.get() {
        return dir.clone();
    }
    if let Ok(p) = std::env::var("SIGNAL_RIG_DIR") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    signal_config_dir().join(crate::instrument::current().dir_name())
}

/// Where one rig keeps its library, and which instrument it is.
///
/// Everything the rig reads and writes goes through its home: the library
/// directory, the shipped defaults a fresh one is seeded from (the guitar's
/// or the bass's), its MIDI learning, its audio rig. Two homes are two
/// rigs that never see each other's files, so a host can run a guitar rig
/// and a bass rig side by side in one process.
///
/// The process has a home of its own ([`process_default`](Self::process_default)):
/// the instrument it was started as or switched to, and [`rig_dir`]. That
/// is the standalone app's, and what every function without a home of its
/// own reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RigHome {
    instrument: Instrument,
    dir: PathBuf,
    /// The home as it was made — what [`with_instrument`](Self::with_instrument)
    /// works from, so a switch there and back lands in the same directory.
    origin: (Instrument, PathBuf),
    /// The process's own home: its directory follows the process-wide
    /// instrument, [`use_rig_dir`] and `SIGNAL_RIG_DIR`, and a switch of
    /// instrument is the process's (`instrument::set`).
    process: bool,
}

impl RigHome {
    /// A rig of `instrument` playing from `dir`, independent of the
    /// process's own library and instrument. Seeded from `instrument`'s
    /// shipped defaults the first time it is read.
    #[must_use]
    pub fn new(instrument: Instrument, dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        adopt_book(&dir);
        Self {
            instrument,
            origin: (instrument, dir.clone()),
            dir,
            process: false,
        }
    }

    /// The process's own home, as it is now: [`crate::instrument::current`]
    /// in [`rig_dir`].
    #[must_use]
    pub fn process_default() -> Self {
        let instrument = crate::instrument::current();
        let dir = rig_dir();
        adopt_book(&dir);
        Self {
            instrument,
            origin: (instrument, dir.clone()),
            dir,
            process: true,
        }
    }

    /// The instrument this rig is.
    #[must_use]
    pub const fn instrument(&self) -> Instrument {
        self.instrument
    }

    /// The library directory.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Whether this is the process's own home.
    #[must_use]
    pub const fn is_process_default(&self) -> bool {
        self.process
    }

    /// The same home playing `to` instead. The process's own home follows
    /// the process (call it after `instrument::set`). One with a directory
    /// of its own plays `to` from the directory it was made with when that
    /// was `to`'s; otherwise from the sibling named for `to` when that
    /// directory is named for its instrument (`<parent>/bass-rig` beside
    /// `<parent>/rig`), else from a folder named for `to` inside it. Either
    /// way the two instruments' libraries never mix, and a switch there and
    /// back comes home.
    #[must_use]
    pub fn with_instrument(&self, to: Instrument) -> Self {
        if self.process {
            return Self::process_default();
        }
        let (made_as, made_in) = &self.origin;
        let sibling_of = made_in
            .parent()
            .filter(|_| made_in.file_name().is_some_and(|n| n == made_as.dir_name()));
        let dir = if *made_as == to {
            made_in.clone()
        } else {
            sibling_of.map_or_else(
                || made_in.join(to.dir_name()),
                |parent| parent.join(to.dir_name()),
            )
        };
        adopt_book(&dir);
        Self {
            instrument: to,
            dir,
            origin: self.origin.clone(),
            process: false,
        }
    }

    /// Its audio rig: the interface calibration and routing it keeps
    /// (`Guitar Rig`, `Bass Rig`).
    #[must_use]
    pub fn audio_rig_name(&self) -> &'static str {
        self.instrument.audio_rig_name()
    }

    /// Its MIDI learning: the process's own keeps it under the config root
    /// by instrument, as it always has; a home of its own, in its directory.
    #[must_use]
    pub fn midi_learn(&self) -> signal_rig_host::midi_learn::MidiLearn {
        if self.process {
            signal_rig_host::midi_learn::MidiLearn::load(self.instrument.id())
        } else {
            signal_rig_host::midi_learn::MidiLearn::load_in(self.dir.clone())
        }
    }

    /// The styx store over the library directory.
    #[must_use]
    pub fn store(&self) -> StyxDir {
        StyxDir::new(self.dir.clone())
    }

    /// The instrument's shipped library.
    pub(crate) const fn shipped(&self) -> &'static Shipped {
        match self.instrument {
            Instrument::Guitar => &GUITAR,
            Instrument::Bass => &BASS,
        }
    }

    /// The store, or `None` when this run does not persist.
    ///
    /// Every save goes through here rather than checking the flag itself:
    /// there is then no way to write to the library without having asked
    /// whether this run is allowed to, including from a save function
    /// nobody has written yet.
    fn writable_store(&self) -> Option<StyxDir> {
        if rig_is_ephemeral() {
            tracing::debug!("ephemeral run — skipping library write");
            return None;
        }
        Some(self.store())
    }

    /// Write any default NAM model missing from `<dir>/models/` (and frozen
    /// Core missing from `<dir>/frozen/`).
    pub(crate) fn seed_models(&self) {
        seed_files(&self.dir.join("models"), self.shipped().models);
        seed_files(&self.dir.join("frozen"), self.shipped().frozen);
    }

    /// The profiles directory.
    #[must_use]
    pub fn profiles_dir(&self) -> PathBuf {
        self.dir.join(PROFILES_DIR)
    }

    /// The store over [`PROFILES_DIR`]. NAM paths still resolve against the
    /// rig directory (`models/…`), so resolving goes through
    /// [`store`](Self::store), not this.
    #[must_use]
    pub fn profiles_store(&self) -> StyxDir {
        StyxDir::new(self.profiles_dir())
    }
}

/// Whether an environment flag is set to something meaning "yes".
fn env_flag(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| {
        let v = v.trim();
        !(v.is_empty() || v == "0" || v.eq_ignore_ascii_case("false"))
    })
}

/// Run without persisting anything: every save is a no-op (`SIGNAL_RIG_EPHEMERAL`).
///
/// Reads still work, so the rig plays the real library — this is not a
/// sandbox, it is a rig that forgets. That is what a benchmark or a test
/// wants: the actual profile, the actual captures, and no trace afterwards.
///
/// The alternative, pointing [`SIGNAL_RIG_DIR`](rig_dir) at a temp
/// directory, gives isolation but a *different* rig, which is the wrong
/// instrument to measure. Both together give an isolated rig that also
/// leaves its seed directory alone.
#[must_use]
pub fn rig_is_ephemeral() -> bool {
    // Said outright either way, that wins: a design run with
    // `SIGNAL_RIG_EPHEMERAL=0` saves (into whatever `SIGNAL_RIG_DIR` says) —
    // how a test drives the library end to end without an audio device.
    match std::env::var("SIGNAL_RIG_EPHEMERAL") {
        Ok(_) => env_flag("SIGNAL_RIG_EPHEMERAL"),
        Err(_) => rig_is_design(),
    }
}

/// Run with the output muted, processing everything (`SIGNAL_RIG_SILENT`).
///
/// Mute is a −96 dB trim on the master, applied after the chain, so every
/// block still runs and every measurement is the real one. A benchmark must
/// not be quieter *to compute* than the rig it stands in for.
#[must_use]
pub fn rig_is_silent() -> bool {
    env_flag("SIGNAL_RIG_SILENT")
}

/// Run the GUI with no audio at all (`SIGNAL_RIG_DESIGN`).
///
/// For designing the interface: the profile loads, the whole UI renders, the
/// meters move, and nothing is opened — no audio device, no MIDI node, no DSP.
/// Several instances can therefore run side by side, which is the point: a
/// device is exclusive and two rigs fighting over one interface is not a thing
/// you can lay out a screen against.
///
/// Implies [`rig_is_ephemeral`]: a run with no audio has nothing worth saving,
/// and a design session must not move the player's position.
#[must_use]
pub fn rig_is_design() -> bool {
    env_flag("SIGNAL_RIG_DESIGN")
}

/// The DI player's recordings (`di.styx`, their files in `di/`).
#[derive(Clone, Debug, Default, Facet)]
pub struct DiLib {
    #[facet(default)]
    pub clips: Vec<crate::proto::DiClip>,
}

impl RigHome {
    /// The recorded DI clips.
    #[must_use]
    pub fn di_lib(&self) -> DiLib {
        self.store().read::<DiLib>("di.styx").unwrap_or_default()
    }

    /// Write `di.styx`.
    pub fn save_di_lib(&self, lib: &DiLib) {
        let Some(store) = self.writable_store() else {
            return;
        };
        config_watch::write_guarded(&store.dir().join("di.styx"), lib);
    }

    /// Where recording `id` lives: the library's `di/`, or a scratch folder
    /// on a run that writes nothing.
    #[must_use]
    pub fn di_clip_path(&self, id: &str) -> PathBuf {
        let dir = if rig_is_ephemeral() {
            std::env::temp_dir().join("signal-di")
        } else {
            self.dir.join("di")
        };
        dir.join(format!("{id}.wav"))
    }
}

// Wrapper structs: styx serialises a struct per file.
#[derive(Clone, Debug, Facet)]
pub struct DrivePresetLib {
    pub presets: Vec<DrivePresetDef>,
}

#[derive(Clone, Debug, Facet)]
pub struct SongLib {
    pub songs: Vec<SongDef>,
}

/// What the library knows of its songs beyond `songs.styx`
/// (`song-info.styx`): each song's colour, artist and genre, and the
/// collections songs are gathered into ("Church"…). A file of its own so
/// nothing that rewrites the song list can drop it, and a library from
/// before it simply has none yet.
#[derive(Clone, Debug, Default, Facet)]
pub struct SongInfoLib {
    #[facet(default)]
    pub songs: Vec<SongInfoDef>,
    #[facet(default)]
    pub collections: Vec<CollectionDef>,
}

/// One song's details, by name.
#[derive(Clone, Debug, Default, Facet)]
pub struct SongInfoDef {
    pub song: String,
    /// `#rrggbb`; empty: one picked from its name.
    #[facet(default)]
    pub colour: String,
    #[facet(default)]
    pub artist: String,
    #[facet(default)]
    pub genre: String,
}

/// A named group of songs; a song can be in any number of them.
#[derive(Clone, Debug, Default, Facet)]
pub struct CollectionDef {
    pub name: String,
    /// `#rrggbb`.
    #[facet(default)]
    pub colour: String,
    #[facet(default)]
    pub songs: Vec<String>,
}

impl SongInfoLib {
    /// A song's details (matched without regard to case).
    #[must_use]
    pub fn of(&self, song: &str) -> Option<&SongInfoDef> {
        self.songs
            .iter()
            .find(|s| s.song.eq_ignore_ascii_case(song))
    }

    /// A song's details to change, made when it has none.
    pub fn of_mut(&mut self, song: &str) -> &mut SongInfoDef {
        if let Some(i) = self
            .songs
            .iter()
            .position(|s| s.song.eq_ignore_ascii_case(song))
        {
            return &mut self.songs[i];
        }
        self.songs.push(SongInfoDef {
            song: song.to_string(),
            ..SongInfoDef::default()
        });
        self.songs.last_mut().expect("just pushed")
    }
}

#[derive(Clone, Debug, Facet)]
pub struct SetlistLib {
    pub setlists: Vec<SetlistDef>,
}

#[derive(Clone, Debug, Facet)]
pub struct KeymapLib {
    pub bindings: Vec<KeyBindingDef>,
}

/// The rig's last-active position (`last-state.styx`) — flushed by the
/// meter pump on patch/song/part/setlist/tempo changes.
///
/// Restored on the next open, so a crash restart mid-set lands back on the same song
/// instead of song 1 at 120 BPM. Indices/names are re-validated against
/// the (possibly edited) library on restore.
#[derive(Clone, Debug, Default, Facet)]
pub struct LastState {
    #[facet(default)]
    pub setlist_index: u32,
    #[facet(default)]
    pub song_index: u32,
    #[facet(default)]
    pub part_index: u32,
    /// Active patch by name; empty = none saved.
    #[facet(default)]
    pub active_patch: String,
    /// Tapped/recalled tempo; 0 = none saved.
    #[facet(default)]
    pub tempo_bpm: f32,
    /// The active profile by name; empty = the first one.
    #[facet(default)]
    pub profile: String,
    /// The perform mode (0 Preset, 1 Profile, 2 Setlist) — songs are live
    /// only in Setlist, so a restart mid-set must land back in it.
    #[facet(default = 1)]
    pub perform_mode: u32,
    /// The whole rig's output trim (dB) — the main fader. −6 dB by default:
    /// headroom for the system the rig feeds.
    #[facet(default = -6.0_f32)]
    pub master_trim_db: f32,
    /// The phones' faders (positions, 0–1, unity at 0.75): overall, your
    /// guitar, the incoming mix.
    #[facet(default = 0.75_f32)]
    pub phones_volume: f32,
    #[facet(default = 0.75_f32)]
    pub phones_guitar: f32,
    #[facet(default = 0.75_f32)]
    pub phones_mix: f32,
    /// The main output muted (the phones keep playing).
    #[facet(default)]
    pub main_mute: bool,
}

/// Everything loaded from the rig directory.
#[derive(Clone, Debug)]
pub struct RigLibrary {
    /// The active profile — the one the rig plays.
    pub profile: ProfileDef,
    /// Every profile in `profiles/`, the active one included, by name.
    pub profiles: Vec<ProfileDef>,
    pub drive_presets: Vec<DrivePresetDef>,
    pub songs: Vec<SongDef>,
    /// `song-info.styx`: song colours, artists, genres, collections.
    pub song_info: SongInfoLib,
    /// `setup.styx`: the guitars, audio rigs and MIDI controllers.
    pub setup: crate::proto::SetupModel,
    pub setlists: Vec<SetlistDef>,
    pub midi_map: MidiMapDef,
    pub keymap: Vec<KeyBindingDef>,
}

// The in-repo default config, embedded so installed binaries can seed a
// fresh machine without a checkout.
pub(crate) const DEFAULT_PROFILE: &str = include_str!("../default-config/profile.styx");
pub(crate) const DEFAULT_DRIVE_PRESETS: &str = include_str!("../default-config/drive-presets.styx");
const DEFAULT_SONGS: &str = include_str!("../default-config/songs.styx");
const DEFAULT_SETLISTS: &str = include_str!("../default-config/setlists.styx");
const DEFAULT_MIDI: &str = include_str!("../default-config/midi.styx");
const DEFAULT_KEYMAP: &str = include_str!("../default-config/keymap.styx");
// The composition libraries a shipped profile is built from (the Blues
// profile's "John Mayer" Core, its Time module, its blocks).
const DEFAULT_MODULES: &str = include_str!("../default-config/modules.styx");
const DEFAULT_PRESETS: &str = include_str!("../default-config/presets.styx");
const DEFAULT_BLOCKS: &str = include_str!("../default-config/blocks.styx");
const DEFAULT_TONES: &str = include_str!("../default-config/tones.styx");

// The shipped library's files — `DEFAULT_MODELS` (captures and cab IRs,
// `models/`), `DEFAULT_FROZEN` (frozen Cores, `frozen/`), `DEFAULT_PROFILES`
// (`profiles/<file>`, seeded into a library that has never had them — see
// [`seed_profiles`]) — listed by build.rs from `default-config/`.
include!(concat!(env!("OUT_DIR"), "/default_files.rs"));

/// One instrument's shipped library: what a fresh install of it is seeded
/// with. The `DEFAULT_*` consts are the guitar's (and what the guitar's
/// tests read); a rig reads its own instrument's through
/// [`RigHome::shipped`].
pub(crate) struct Shipped {
    pub profile: &'static str,
    pub drive_presets: &'static str,
    songs: &'static str,
    setlists: &'static str,
    midi: &'static str,
    keymap: &'static str,
    modules: &'static str,
    presets: &'static str,
    blocks: &'static str,
    tones: &'static str,
    factory: &'static str,
    models: &'static [(&'static str, &'static [u8])],
    frozen: &'static [(&'static str, &'static [u8])],
    profiles: &'static [(&'static str, &'static str)],
}

static GUITAR: Shipped = Shipped {
    profile: DEFAULT_PROFILE,
    drive_presets: DEFAULT_DRIVE_PRESETS,
    songs: DEFAULT_SONGS,
    setlists: DEFAULT_SETLISTS,
    midi: DEFAULT_MIDI,
    keymap: DEFAULT_KEYMAP,
    modules: DEFAULT_MODULES,
    presets: DEFAULT_PRESETS,
    blocks: DEFAULT_BLOCKS,
    tones: DEFAULT_TONES,
    factory: FACTORY,
    models: DEFAULT_MODELS,
    frozen: DEFAULT_FROZEN,
    profiles: DEFAULT_PROFILES,
};

static BASS: Shipped = Shipped {
    profile: include_str!("../default-config-bass/profile.styx"),
    drive_presets: include_str!("../default-config-bass/drive-presets.styx"),
    songs: include_str!("../default-config-bass/songs.styx"),
    setlists: include_str!("../default-config-bass/setlists.styx"),
    midi: include_str!("../default-config-bass/midi.styx"),
    keymap: include_str!("../default-config-bass/keymap.styx"),
    modules: include_str!("../default-config-bass/modules.styx"),
    presets: include_str!("../default-config-bass/presets.styx"),
    blocks: include_str!("../default-config-bass/blocks.styx"),
    tones: include_str!("../default-config-bass/tones.styx"),
    factory: include_str!("../default-config-bass/factory.txt"),
    models: BASS_MODELS,
    frozen: BASS_FROZEN,
    profiles: BASS_PROFILES,
};

/// The profile a rig plays when nothing has chosen one (and the one a
/// library that newly gains it switches to, once): the worship rig — on a
/// laptop or an iPad at a service — and Blues on a phone.
#[must_use]
pub fn default_profile_name() -> &'static str {
    if cfg!(target_os = "ios") && !is_ipad() {
        "Blues"
    } else {
        "Worship"
    }
}

/// Whether this is an iPad: the device's model (`hw.machine`, `iPad14,3`),
/// or under the simulator, the model it simulates.
#[cfg(target_os = "ios")]
fn is_ipad() -> bool {
    if let Ok(m) = std::env::var("SIMULATOR_MODEL_IDENTIFIER") {
        return m.starts_with("iPad");
    }
    let mut buf = [0u8; 64];
    let mut len = buf.len();
    // SAFETY: a NUL-terminated name, and a buffer with its length.
    let ok = unsafe {
        libc::sysctlbyname(
            c"hw.machine".as_ptr(),
            buf.as_mut_ptr().cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    } == 0;
    ok && buf[..len.min(buf.len())].starts_with(b"iPad")
}

#[cfg(not(target_os = "ios"))]
fn is_ipad() -> bool {
    false
}

/// Write each `(name, bytes)` into `dir` unless it is there already.
fn seed_files(dir: &std::path::Path, files: &[(&str, &[u8])]) {
    for (name, bytes) in files {
        let path = dir.join(name);
        if path.exists() {
            continue;
        }
        if let Err(e) = std::fs::create_dir_all(dir) {
            tracing::warn!("rig library: cannot create {}: {e}", dir.display());
            return;
        }
        if let Err(e) = std::fs::write(&path, bytes) {
            tracing::warn!("rig library: seed {name} failed: {e}");
        } else {
            tracing::info!("rig library: seeded {name}");
        }
    }
}

/// Where the profiles live, one styx file each.
pub const PROFILES_DIR: &str = "profiles";

/// The file a profile is saved as: its name, lowercased, with anything that
/// is not a letter or digit folded to `-` — so "Rock (Live)" is
/// `rock-live.styx` and a name can never climb out of the directory.
#[must_use]
pub fn profile_file(name: &str) -> String {
    let mut slug = String::new();
    for c in name.trim().chars() {
        if c.is_alphanumeric() {
            slug.extend(c.to_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    format!("{}.styx", if slug.is_empty() { "profile" } else { slug })
}

/// The library at `dir` as its book, when the app has turned books on
/// (`signal_rig_host::book::enable`): the rig's reads and writes go to it.
fn adopt_book(dir: &std::path::Path) {
    #[cfg(not(target_arch = "wasm32"))]
    let _ = signal_rig_host::book::adopt(dir);
    #[cfg(target_arch = "wasm32")]
    let _ = dir;
}

/// Whether the shipped defaults may be written into the library at `dir`:
/// always for a directory; for a book, once it may
/// (`signal_rig_host::book::RigBook::may_seed` — an account's book waits
/// for the account's library, or defaults would land on top of it).
fn may_seed(dir: &std::path::Path) -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some((book, _)) = signal_rig_host::book::for_tree(dir) {
        return book.may_seed();
    }
    let _ = dir;
    true
}

/// A file that does not parse: said loudly, in the log and the reload log,
/// and otherwise left alone — never reseeded, never saved over.
pub(crate) fn report_bad(path: &std::path::Path, err: &str) {
    let line = format!(
        "{}: does not parse — kept the running state, the file is left as it is: {err}",
        config_watch::display_name(path)
    );
    tracing::error!("{line}");
    config_watch::log_line(&line);
}

/// Read `file`, seeding it from the embedded default when it is missing.
/// One that is there and does not parse plays the default in memory and is
/// left alone — it is never reseeded, and no save overwrites it until it
/// parses again.
fn read_or_seed<T: for<'a> Facet<'a>>(
    store: &StyxDir,
    file: &str,
    seed: &str,
    fallback: impl FnOnce() -> T,
) -> T {
    let path = store.dir().join(file);
    match config_watch::read_tracked::<T>(&path) {
        Read::Ok(v) => v,
        Read::Bad(e) => {
            report_bad(&path, &e);
            facet_styx::from_str::<T>(seed).unwrap_or_else(|_| fallback())
        }
        Read::Missing => {
            let v = store.read_or_seed(file, seed, fallback);
            // Seeded: remember it as read, so the first save is not taken
            // for an overwrite of somebody else's file.
            if let Ok(text) = std::fs::read_to_string(&path) {
                config_watch::note_read(&path, &text, &v);
            }
            v
        }
    }
}

impl RigHome {
    /// Resolve a profile's rig-dir-relative capture paths, as a load does.
    pub(crate) fn resolve_profile(&self, profile: &mut ProfileDef) {
        let store = self.store();
        for preset in &mut profile.presets {
            store.resolve(&mut preset.nam);
        }
    }

    /// Resolve the drive presets' capture paths, as a load does.
    pub(crate) fn resolve_drive_presets(&self, presets: &mut [DrivePresetDef]) {
        let store = self.store();
        for dp in presets {
            for option in &mut dp.options {
                store.resolve(&mut option.nam);
            }
        }
    }
}

/// Put every song's patches on `profile` (tagged with their song), for the
/// songs played on it — those naming it, and those naming no profile (they
/// play on whatever is loaded).
pub(crate) fn attach_song_patches(profile: &mut ProfileDef, songs: &[SongDef]) {
    for song in songs {
        if !song.profile.is_empty() && !song.profile.eq_ignore_ascii_case(&profile.name) {
            continue;
        }
        for p in &song.patches {
            if profile
                .patches
                .iter()
                .any(|x| x.name.eq_ignore_ascii_case(&p.name))
            {
                tracing::warn!(song = %song.name, patch = %p.name, "rig library: a song patch shares a name with a profile patch — skipped");
                continue;
            }
            let mut p = p.clone();
            p.song.clone_from(&song.name);
            profile.patches.push(p);
        }
    }
}

impl RigHome {
    /// Every profile in `profiles/`, sorted by name.
    ///
    /// A rig from before there were several profiles has one `profile.styx`
    /// instead. That file becomes the first profile: written into `profiles/`
    /// and renamed `profile.styx.migrated`, so a hand edit to the old file is
    /// not silently ignored — it is plainly not the file any more. A run that
    /// may not write (design, ephemeral) reads it in place and leaves it alone.
    fn load_profiles(&self, store: &StyxDir) -> Vec<ProfileDef> {
        let dir = self.profiles_store();
        let mut files: Vec<String> = profile_files(dir.dir());
        files.sort();
        let mut profiles: Vec<ProfileDef> = files
            .iter()
            .filter_map(|f| {
                let path = dir.dir().join(f);
                match config_watch::read_tracked::<ProfileDef>(&path) {
                    Read::Ok(p) => Some(p),
                    Read::Missing => None,
                    Read::Bad(e) => {
                        report_bad(&path, &e);
                        None
                    }
                }
            })
            .collect();
        // Only a rig with no profile files at all is a legacy one: profiles
        // that are there and do not parse are somebody's work, and seeding a
        // default into their place could overwrite one of them.
        if files.is_empty() {
            let legacy = read_or_seed(store, "profile.styx", self.shipped().profile, worship_def);
            if self.writable_store().is_some() {
                config_watch::write_guarded(&dir.dir().join(profile_file(&legacy.name)), &legacy);
                let old = store.dir().join("profile.styx");
                if let Err(e) = std::fs::rename(&old, old.with_extension("styx.migrated")) {
                    tracing::warn!("rig library: cannot retire {}: {e}", old.display());
                }
                tracing::info!("rig library: profile.styx moved to {PROFILES_DIR}/");
            }
            profiles.push(legacy);
        }
        profiles.sort_by_key(|p| p.name.to_lowercase());
        profiles
    }
}

/// The profile files in `dir`: a book's, and those of its old directory not
/// moved in yet (they move in as they are read); else the directory's.
fn profile_files(dir: &std::path::Path) -> Vec<String> {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some((book, prefix)) = signal_rig_host::book::for_tree(dir) {
        return book
            .names_with_files(&prefix)
            .into_iter()
            .filter_map(|n| n.strip_prefix(&prefix).map(str::to_owned))
            .filter(|n| !n.contains('/'))
            .collect();
    }
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|n| n.ends_with(".styx"))
                .collect()
        })
        .unwrap_or_default()
}

/// The shipped profiles a library has been given, by name, one a line in
/// `profiles/.seeded`: a shipped profile is seeded once, so one the player
/// deletes stays deleted.
const SEEDED_MARKER: &str = ".seeded";

/// The default profile a library has been switched to, in
/// `profiles/.default`: a library opens on [`default_profile_name`] once
/// when it changes (an install from before it, whose last-played profile
/// would otherwise win), then remembers the player's choice.
const DEFAULT_MARKER: &str = ".default";

impl RigHome {
    /// Add each shipped profile (`DEFAULT_PROFILES`) the library has never had
    /// — a fresh install, or one from before it shipped — to `profiles` and to
    /// `profiles/`. Whether the library should open on [`default_profile_name`]
    /// this once: it was just seeded, or the library has not opened on it yet.
    fn seed_profiles(&self, profiles: &mut Vec<ProfileDef>) -> bool {
        let dir = self.profiles_store();
        if !may_seed(dir.dir()) {
            return false;
        }
        let marker = dir.dir().join(SEEDED_MARKER);
        let mut seeded: Vec<String> = std::fs::read_to_string(&marker)
            .map(|t| t.lines().map(str::to_string).collect())
            .unwrap_or_default();
        let mut gained_default = false;
        let mut changed = false;
        for (file, text) in self.shipped().profiles {
            let def: ProfileDef = match facet_styx::from_str(text) {
                Ok(def) => def,
                Err(e) => {
                    tracing::warn!(file, error = %e, "rig library: shipped profile does not parse");
                    continue;
                }
            };
            let had = seeded.iter().any(|n| n.eq_ignore_ascii_case(&def.name));
            let present = profiles
                .iter()
                .any(|p| p.name.eq_ignore_ascii_case(&def.name));
            if !had {
                seeded.push(def.name.clone());
                changed = true;
            }
            // One the library has as the app shipped it takes the newer one.
            if let Some(mine) = profiles
                .iter_mut()
                .find(|p| p.name.eq_ignore_ascii_case(&def.name))
                && factory_update(self.shipped().factory, file, &def.name, &*mine, &def)
            {
                if self.writable_store().is_some() {
                    config_watch::write_guarded(&dir.dir().join(file), &def);
                }
                tracing::info!(profile = %def.name, "rig library: shipped profile updated");
                *mine = def;
                continue;
            }
            if had || present {
                continue;
            }
            if self.writable_store().is_some() {
                config_watch::write_guarded(&dir.dir().join(file), &def);
            }
            tracing::info!(profile = %def.name, "rig library: seeded shipped profile");
            gained_default |= def.name.eq_ignore_ascii_case(default_profile_name());
            profiles.push(def);
        }
        if changed && self.writable_store().is_some() {
            let _ = std::fs::create_dir_all(dir.dir());
            if let Err(e) = std::fs::write(&marker, seeded.join("\n") + "\n") {
                tracing::warn!("rig library: cannot write {}: {e}", marker.display());
            }
        }
        let default_marker = dir.dir().join(DEFAULT_MARKER);
        let opened_on = std::fs::read_to_string(&default_marker).unwrap_or_default();
        if !opened_on
            .trim()
            .eq_ignore_ascii_case(default_profile_name())
            && self.writable_store().is_some()
        {
            gained_default = true;
            let _ = std::fs::create_dir_all(dir.dir());
            if let Err(e) = std::fs::write(&default_marker, format!("{}\n", default_profile_name()))
            {
                tracing::warn!(
                    "rig library: cannot write {}: {e}",
                    default_marker.display()
                );
            }
        }
        profiles.sort_by_key(|p| p.name.to_lowercase());
        gained_default
    }
}

/// The shipped drive pedals a library has been given, by name, one a line
/// (`.seeded-drive-presets`): seeded once, so one the player deletes stays
/// deleted.
const SEEDED_DRIVES_MARKER: &str = ".seeded-drive-presets";

impl RigHome {
    /// Add each shipped drive pedal (`DEFAULT_DRIVE_PRESETS`) a library from
    /// before it shipped has never had — a shipped profile names its pedals,
    /// and a slot whose pedal the library lacks plays nothing ("Empty"). Whether
    /// any was added (the caller saves).
    fn seed_drive_presets(&self, presets: &mut Vec<DrivePresetDef>) -> bool {
        let Some(store) = self.writable_store() else {
            return false;
        };
        let Ok(shipped) = facet_styx::from_str::<DrivePresetLib>(self.shipped().drive_presets)
        else {
            tracing::warn!("rig library: shipped drive presets do not parse");
            return false;
        };
        if !may_seed(store.dir()) {
            return false;
        }
        let marker = store.dir().join(SEEDED_DRIVES_MARKER);
        let mut seeded: Vec<String> = std::fs::read_to_string(&marker)
            .map(|t| t.lines().map(str::to_string).collect())
            .unwrap_or_default();
        let mut added = false;
        let mut changed = false;
        for dp in shipped.presets {
            if seeded.iter().any(|n| n.eq_ignore_ascii_case(&dp.name)) {
                continue;
            }
            seeded.push(dp.name.clone());
            changed = true;
            if !presets
                .iter()
                .any(|p| p.name.eq_ignore_ascii_case(&dp.name))
            {
                tracing::info!(pedal = %dp.name, "rig library: seeded shipped drive preset");
                presets.push(dp);
                added = true;
            }
        }
        if changed {
            if let Err(e) = std::fs::write(&marker, seeded.join("\n") + "\n") {
                tracing::warn!("rig library: cannot write {}: {e}", marker.display());
            }
        }
        added
    }
}

impl RigHome {
    /// Add each shipped module preset and block preset a library from before
    /// it shipped has never had, to `modules.styx` and `blocks.styx` — once
    /// (`.seeded-modules`, `.seeded-blocks`), so one the player deletes stays
    /// deleted. A file the library does not have yet is seeded whole on first
    /// read (see `read_compositions`); this is for the one it already has.
    fn seed_compositions(&self) {
        let Some(store) = self.writable_store() else {
            return;
        };
        seed_entries::<crate::compose::ModuleLib, crate::compose::ModulePresetDef>(
            &store,
            self.shipped().factory,
            crate::compose::MODULES_FILE,
            self.shipped().modules,
            ".seeded-modules",
            |l| &mut l.presets,
            |p| format!("{}\t{}", p.module, p.name),
        );
        seed_entries::<crate::compose::BlockLib, crate::compose::BlockPresetDef>(
            &store,
            self.shipped().factory,
            crate::compose::BLOCKS_FILE,
            self.shipped().blocks,
            ".seeded-blocks",
            |l| &mut l.presets,
            |p| format!("{}\t{}", p.block_type, p.name),
        );
        seed_entries::<crate::compose::PresetLib, crate::compose::RigPresetDef>(
            &store,
            self.shipped().factory,
            crate::compose::PRESETS_FILE,
            self.shipped().presets,
            ".seeded-presets",
            |l| &mut l.presets,
            |p| p.name.clone(),
        );
        seed_entries::<crate::compose::ToneLib, crate::compose::ToneDef>(
            &store,
            self.shipped().factory,
            crate::compose::TONES_FILE,
            self.shipped().tones,
            ".seeded-tones",
            |l| &mut l.tones,
            |t| t.name.clone(),
        );
    }
}

impl RigHome {
    /// Add each shipped song and setlist a library from before it shipped has
    /// never had — once (`.seeded-songs`, `.seeded-setlists`), so one the
    /// player deletes stays deleted. A set the app ships (a worship night's
    /// order) reaches a phone that already has its own songs.
    fn seed_songs_and_setlists(&self) {
        let Some(store) = self.writable_store() else {
            return;
        };
        let store = &store;
        seed_entries::<SongLib, crate::profiles::SongDef>(
            store,
            self.shipped().factory,
            "songs.styx",
            self.shipped().songs,
            ".seeded-songs",
            |l| &mut l.songs,
            |s| s.name.clone(),
        );
        seed_entries::<SetlistLib, crate::profiles::SetlistDef>(
            store,
            self.shipped().factory,
            "setlists.styx",
            self.shipped().setlists,
            ".seeded-setlists",
            |l| &mut l.setlists,
            |s| s.name.clone(),
        );
    }
}

/// Every revision the app has shipped of each shipped entry, as
/// fingerprints — `file`, key and [`fingerprint`] a line, tab-separated
/// (a key's own tab written ` :: `). An entry still matching one is the
/// factory's, untouched by the player, so a newer shipped version of it
/// replaces it ([`factory_update`]); one they changed is theirs and stays.
/// `ship-library.sh` records the shipped files here before it changes them.
const FACTORY: &str = include_str!("../default-config/factory.txt");

/// An entry as text, its capture and IR paths cut to their file names — so
/// a library's `/Users/…/x.nam` and the shipped `models/x.nam` read the same
/// — hashed (FNV-1a, 64-bit, hex).
pub(crate) fn fingerprint<T: for<'a> Facet<'a>>(entry: &T) -> String {
    let text = facet_styx::to_string(entry).unwrap_or_default();
    let mut out = String::with_capacity(text.len());
    let mut token = String::new();
    let mut quoted = false;
    let flush = |token: &mut String, out: &mut String| {
        let lower = token.to_lowercase();
        if (lower.ends_with(".nam") || lower.ends_with(".wav")) && token.contains('/') {
            out.push_str(token.rsplit('/').next().unwrap_or(token));
        } else {
            out.push_str(token);
        }
        token.clear();
    };
    for c in text.chars() {
        let boundary = if quoted {
            c == '"'
        } else {
            c == '"' || c.is_whitespace() || ",(){}".contains(c)
        };
        if boundary {
            flush(&mut token, &mut out);
            out.push(c);
            if c == '"' {
                quoted = !quoted;
            }
        } else {
            token.push(c);
        }
    }
    flush(&mut token, &mut out);
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in out.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// The [`FACTORY`] line for an entry.
pub(crate) fn factory_line(file: &str, key: &str, fingerprint: &str) -> String {
    format!("{file}\t{}\t{fingerprint}", key.replace('\t', " :: "))
}

/// Whether `mine` should take the shipped `ship`: it differs, and is still
/// one of the versions the app shipped before (by the instrument's
/// `factory` record, [`FACTORY`] for the guitar).
fn factory_update<T: for<'a> Facet<'a>>(
    factory: &str,
    file: &str,
    key: &str,
    mine: &T,
    ship: &T,
) -> bool {
    let have = fingerprint(mine);
    have != fingerprint(ship) && factory.lines().any(|l| l == factory_line(file, key, &have))
}

/// [`RigHome::seed_compositions`] for one file: the shipped entries (by `key`) not in
/// the library's file nor in its `marker` are added and written back, and
/// the ones it still has as shipped take the shipped version now
/// ([`factory_update`] against `factory`, the instrument's record).
fn seed_entries<L, E>(
    store: &StyxDir,
    factory: &str,
    file: &str,
    shipped: &str,
    marker: &str,
    list: impl Fn(&mut L) -> &mut Vec<E>,
    key: impl Fn(&E) -> String,
) where
    L: for<'a> Facet<'a>,
    E: Clone + for<'a> Facet<'a>,
{
    let path = store.dir().join(file);
    if !may_seed(store.dir()) {
        return;
    }
    let Read::Ok(mut lib) = config_watch::read_tracked::<L>(&path) else {
        return;
    };
    let Ok(mut ship) = facet_styx::from_str::<L>(shipped) else {
        tracing::warn!(file, "rig library: shipped compositions do not parse");
        return;
    };
    let marker = store.dir().join(marker);
    let mut seeded: Vec<String> = std::fs::read_to_string(&marker)
        .map(|t| t.lines().map(str::to_string).collect())
        .unwrap_or_default();
    let have: Vec<String> = list(&mut lib)
        .iter()
        .map(&key)
        .map(|k| k.to_lowercase())
        .collect();
    let mut added = 0usize;
    for e in list(&mut ship).iter() {
        let k = key(e).to_lowercase();
        if seeded.contains(&k) {
            continue;
        }
        seeded.push(k.clone());
        if !have.contains(&k) {
            list(&mut lib).push(e.clone());
            added += 1;
        }
    }
    let mut updated = 0usize;
    for e in list(&mut ship).iter() {
        let k = key(e);
        if let Some(mine) = list(&mut lib)
            .iter_mut()
            .find(|m| key(m).eq_ignore_ascii_case(&k))
            && factory_update(factory, file, &k, &*mine, e)
        {
            *mine = e.clone();
            updated += 1;
        }
    }
    if added > 0 || updated > 0 {
        config_watch::write_guarded(&path, &lib);
        tracing::info!(file, added, updated, "rig library: seeded shipped presets");
    }
    if let Err(e) = std::fs::write(&marker, seeded.join("\n") + "\n") {
        tracing::warn!("rig library: cannot write {}: {e}", marker.display());
    }
}

impl RigHome {
    /// Write `profile` (its own patches only) to `profiles/<file>.styx`.
    fn save_profile_file(&self, store: &StyxDir, mut profile: ProfileDef) {
        for preset in &mut profile.presets {
            store.relativize(&mut preset.nam);
        }
        config_watch::write_guarded(
            &self
                .profiles_store()
                .dir()
                .join(profile_file(&profile.name)),
            &profile,
        );
    }
}

static COMPOSITIONS_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl RigHome {
    /// Load the library, bootstrapping any missing file (and the NAM
    /// models the defaults reference) from the embedded in-repo default
    /// config, so the directory is always complete.
    #[must_use]
    pub fn load_or_bootstrap(&self) -> RigLibrary {
        self.seed_models();
        let store = self.store();
        let mut profiles = self.load_profiles(&store);
        let gained_default = self.seed_profiles(&mut profiles);
        self.split_core_once(&store, &mut profiles);
        let mut drive_presets = read_or_seed::<DrivePresetLib>(
            &store,
            "drive-presets.styx",
            self.shipped().drive_presets,
            || DrivePresetLib {
                presets: drive_presets(),
            },
        )
        .presets;
        if self.seed_drive_presets(&mut drive_presets) {
            self.save_drive_presets(&drive_presets);
        }
        self.seed_compositions();
        self.seed_songs_and_setlists();
        let songs =
            read_or_seed::<SongLib>(&store, "songs.styx", self.shipped().songs, || SongLib {
                songs: song_library(),
            })
            .songs;
        let setlists =
            read_or_seed::<SetlistLib>(&store, "setlists.styx", self.shipped().setlists, || {
                SetlistLib {
                    setlists: default_setlists(),
                }
            })
            .setlists;
        let midi_map =
            read_or_seed::<MidiMapDef>(&store, "midi.styx", self.shipped().midi, default_midi_map);
        let keymap =
            read_or_seed::<KeymapLib>(&store, "keymap.styx", self.shipped().keymap, || KeymapLib {
                bindings: default_keymap(),
            })
            .bindings;
        for profile in &mut profiles {
            for preset in &mut profile.presets {
                store.resolve(&mut preset.nam);
            }
            attach_song_patches(profile, &songs);
        }
        // The profile last played; the default when none was, or when the
        // library has just gained it (once — an install from before it
        // shipped opens on it the first time, then remembers the choice).
        let wanted = self
            .load_last_state()
            .map(|s| s.profile)
            .filter(|p| !p.is_empty() && !gained_default)
            .unwrap_or_else(|| default_profile_name().to_string());
        let profile = profiles
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(&wanted))
            .or_else(|| profiles.first())
            .cloned()
            .unwrap_or_else(worship_def);
        for dp in &mut drive_presets {
            for option in &mut dp.options {
                store.resolve(&mut option.nam);
            }
        }
        // A library with no song details yet starts with one collection,
        // Church, empty, for the player to fill.
        let song_info = store
            .read::<SongInfoLib>("song-info.styx")
            .unwrap_or_else(|| SongInfoLib {
                songs: Vec::new(),
                collections: vec![CollectionDef {
                    name: "Church".to_string(),
                    colour: "#f472b6".to_string(),
                    songs: Vec::new(),
                }],
            });
        let setup = store
            .read::<crate::proto::SetupModel>("setup.styx")
            .unwrap_or_else(|| self.seed_setup());
        RigLibrary {
            profile,
            profiles,
            drive_presets,
            songs,
            song_info,
            setup,
            setlists,
            midi_map,
            keymap,
        }
    }

    /// The module-preset and preset libraries every profile composes from
    /// (`modules.styx`, `presets.styx`). Missing reads as empty — a rig with
    /// no compositions yet plays its profiles exactly as written.
    #[must_use]
    pub fn load_compositions(&self) -> crate::compose::Compositions {
        // Cached by the three files' modification stamps: the macro bar
        // resolves its responses on every patch switch, and a switch's
        // follow-up must not parse three styx files to find nothing changed.
        // A save (here or by hand) moves a stamp and the next call re-reads.
        // One entry per library directory, so rigs side by side do not
        // evict each other's.
        type Stamp = Vec<Option<(std::time::SystemTime, u64)>>;
        static CACHE: std::sync::Mutex<
            Vec<(std::path::PathBuf, Stamp, crate::compose::Compositions)>,
        > = std::sync::Mutex::new(Vec::new());
        // More directories than any process plays from; the oldest goes.
        const KEEP: usize = 8;
        let dir = self.dir.clone();
        // A book's compositions change with the book (another device's
        // edit included): its generation is the stamp.
        #[cfg(not(target_arch = "wasm32"))]
        let book_stamp = signal_rig_host::book::for_dir(&dir)
            .map(|b| vec![Some((std::time::UNIX_EPOCH, b.generation()))]);
        #[cfg(target_arch = "wasm32")]
        let book_stamp: Option<Stamp> = None;
        let stamp: Stamp = book_stamp.unwrap_or_else(|| {
            [
                crate::compose::MODULES_FILE,
                crate::compose::PRESETS_FILE,
                crate::compose::BLOCKS_FILE,
                crate::compose::TONES_FILE,
            ]
            .iter()
            .map(|f| {
                std::fs::metadata(dir.join(f))
                    .ok()
                    .and_then(|m| Some((m.modified().ok()?, m.len())))
            })
            .collect()
        });
        if let Ok(cache) = CACHE.lock()
            && let Some((_, _, comp)) = cache.iter().find(|(d, st, _)| *d == dir && *st == stamp)
        {
            return comp.clone();
        }
        let last_good = CACHE.lock().ok().and_then(|c| {
            c.iter()
                .find(|(d, _, _)| *d == dir)
                .map(|(_, _, comp)| comp.clone())
        });
        let comp = self.read_compositions(last_good.as_ref());
        COMPOSITIONS_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if let Ok(mut cache) = CACHE.lock() {
            cache.retain(|(d, _, _)| *d != dir);
            if cache.len() >= KEEP {
                cache.remove(0);
            }
            cache.push((dir, stamp, comp.clone()));
        }
        comp
    }

    /// Read the three composition files. One that does not parse keeps what
    /// was last read from it (`last_good`) rather than reading as empty — an
    /// empty module library would rebuild every patch without its amp.
    fn read_compositions(
        &self,
        last_good: Option<&crate::compose::Compositions>,
    ) -> crate::compose::Compositions {
        fn one<T: for<'a> Facet<'a> + Default>(
            home: &RigHome,
            file: &str,
            seed: &str,
        ) -> Result<T, ()> {
            let path = home.dir.join(file);
            match config_watch::read_tracked::<T>(&path) {
                Read::Ok(v) => Ok(v),
                // A library that has never had it gets the shipped one:
                // without it a shipped profile's composed patches resolve
                // to nothing (written where this run may write).
                Read::Missing => {
                    let v: T = facet_styx::from_str(seed).unwrap_or_default();
                    if home.writable_store().is_some() && may_seed(&home.dir) {
                        #[cfg(not(target_arch = "wasm32"))]
                        if let Some((book, name)) = signal_rig_host::book::for_path(&path) {
                            if let Err(e) = book.write(&name, &v) {
                                tracing::warn!(
                                    "rig library: seed {file} into the book failed: {e}"
                                );
                            }
                            return Ok(v);
                        }
                        let _ = std::fs::create_dir_all(&home.dir);
                        if let Err(e) = std::fs::write(&path, seed) {
                            tracing::warn!("rig library: seed {file} failed: {e}");
                        }
                    }
                    Ok(v)
                }
                Read::Bad(e) => {
                    report_bad(&path, &e);
                    Err(())
                }
            }
        }
        let store = self.store();
        let modules = one::<crate::compose::ModuleLib>(
            self,
            crate::compose::MODULES_FILE,
            self.shipped().modules,
        )
        .map(|l| l.presets);
        let fresh_modules = modules.is_ok();
        let mut modules =
            modules.unwrap_or_else(|()| last_good.map(|c| c.modules.clone()).unwrap_or_default());
        if fresh_modules {
            for m in &mut modules {
                for snap in &mut m.snapshots {
                    store.resolve(&mut snap.nam);
                    store.resolve(&mut snap.nam2);
                    store.resolve(&mut snap.cab);
                    store.resolve(&mut snap.cab2);
                }
            }
        }
        let presets = one::<crate::compose::PresetLib>(
            self,
            crate::compose::PRESETS_FILE,
            self.shipped().presets,
        )
        .map(|mut l| {
            for p in &mut l.presets {
                for snap in &mut p.snapshots {
                    store.resolve(&mut snap.frozen_nam);
                    store.resolve(&mut snap.frozen_nam2);
                }
            }
            l.presets
        })
        .unwrap_or_else(|()| last_good.map(|c| c.presets.clone()).unwrap_or_default());
        let blocks = one::<crate::compose::BlockLib>(
            self,
            crate::compose::BLOCKS_FILE,
            self.shipped().blocks,
        )
        .map(|l| l.presets)
        .unwrap_or_else(|()| last_good.map(|c| c.blocks.clone()).unwrap_or_default());
        // The presets: the library's, or the shipped ones until it has its own.
        let tones_path = self.dir.join(crate::compose::TONES_FILE);
        let tones = match config_watch::read_tracked::<crate::compose::ToneLib>(&tones_path) {
            Read::Ok(l) => l.tones,
            Read::Missing => facet_styx::from_str::<crate::compose::ToneLib>(self.shipped().tones)
                .map(|l| l.tones)
                .unwrap_or_default(),
            Read::Bad(e) => {
                report_bad(&tones_path, &e);
                last_good.map(|c| c.tones.clone()).unwrap_or_default()
            }
        };
        crate::compose::Compositions {
            modules,
            presets,
            blocks,
            tones,
        }
    }

    /// Presets become the Core module, once: a library whose presets still
    /// hold time effects, pre effects or chorus and tremolo settings has
    /// them moved out ([`crate::compose::split_core`] — every patch keeps
    /// its sound), the old files kept beside the new as `*.styx.migrated`.
    /// A run that may not write leaves the library as it is (it still
    /// plays: the split changes where settings live, not what plays).
    fn split_core_once(&self, store: &StyxDir, profiles: &mut Vec<ProfileDef>) {
        if self.writable_store().is_none() {
            return;
        }
        let comp = self.load_compositions();
        // The songs' own patches play Core snapshots too: they are carried
        // over with the profiles' (before, they lost what moved out — a
        // WASHED verse's spring reverb).
        let songs_path = store.dir().join("songs.styx");
        let songs = match config_watch::read_quiet::<SongLib>(&songs_path) {
            Read::Ok(l) => Some(l.songs),
            _ => None,
        };
        let (split, split_profiles, split_songs) = crate::compose::split_core_with_songs(
            &comp,
            profiles,
            songs.as_deref().unwrap_or_default(),
        );
        let changed = format!("{:?}", split.presets) != format!("{:?}", comp.presets);
        if !changed {
            return;
        }
        let keep = |path: std::path::PathBuf| {
            if path.exists() {
                if let Err(e) = std::fs::copy(&path, path.with_extension("styx.migrated")) {
                    tracing::warn!(
                        "rig library: cannot keep {} before the Core split: {e}",
                        path.display()
                    );
                }
            }
        };
        for f in [
            crate::compose::PRESETS_FILE,
            crate::compose::MODULES_FILE,
            crate::compose::BLOCKS_FILE,
        ] {
            keep(store.dir().join(f));
        }
        for p in split_profiles.iter() {
            keep(self.profiles_dir().join(profile_file(&p.name)));
        }
        if songs.is_some() {
            keep(songs_path.clone());
        }
        self.save_compositions(&split);
        for p in &split_profiles {
            self.save_profile_file(store, p.clone());
        }
        // Written whole: `save_songs` keeps the file's own song patches,
        // which are what this rewrites.
        if songs.is_some() {
            config_watch::write_guarded(&songs_path, &SongLib { songs: split_songs });
        }
        tracing::info!(
            "rig library: presets are the Core module now — time effects, pre effects, chorus and tremolo moved out (old files kept as *.styx.migrated)"
        );
        *profiles = split_profiles;
    }

    /// Write both composition libraries back.
    pub fn save_compositions(&self, comp: &crate::compose::Compositions) {
        let Some(store) = self.writable_store() else {
            return;
        };
        let mut modules = comp.modules.clone();
        for m in &mut modules {
            for snap in &mut m.snapshots {
                store.relativize(&mut snap.nam);
                store.relativize(&mut snap.nam2);
            }
        }
        let mut presets = comp.presets.clone();
        for p in &mut presets {
            for snap in &mut p.snapshots {
                store.relativize(&mut snap.frozen_nam);
                store.relativize(&mut snap.frozen_nam2);
            }
        }
        let dir = store.dir();
        config_watch::write_guarded(
            &dir.join(crate::compose::MODULES_FILE),
            &crate::compose::ModuleLib { presets: modules },
        );
        // The presets, once there are any (or the file is there to update).
        let tones_path = dir.join(crate::compose::TONES_FILE);
        if !comp.tones.is_empty() || tones_path.exists() {
            config_watch::write_guarded(
                &tones_path,
                &crate::compose::ToneLib {
                    tones: comp.tones.clone(),
                },
            );
        }
        config_watch::write_guarded(
            &dir.join(crate::compose::PRESETS_FILE),
            &crate::compose::PresetLib { presets },
        );
        config_watch::write_guarded(
            &dir.join(crate::compose::BLOCKS_FILE),
            &crate::compose::BlockLib {
                presets: comp.blocks.clone(),
            },
        );
    }

    /// Everything saved about nodes that the profile cannot hold — presets
    /// on a module, presets saved from a tweak, selections with no profile
    /// field. Missing or unreadable reads as empty: a rig with no saved
    /// presets is the normal state, not an error.
    #[must_use]
    pub fn load_node_store(&self) -> crate::node_store::NodeStore {
        // Read per use, so it is always what the rig holds: tracked, so the
        // save that follows a read is not mistaken for an overwrite.
        let path = self.dir.join(crate::node_store::NODE_STORE_FILE);
        match config_watch::read_tracked(&path) {
            Read::Ok(v) => v,
            Read::Missing => crate::node_store::NodeStore::default(),
            Read::Bad(e) => {
                report_bad(&path, &e);
                crate::node_store::NodeStore::default()
            }
        }
    }

    /// Write it back. Best-effort, like the profile: losing a preset is not
    /// worth failing a rig for.
    pub fn save_node_store(&self, nodes: &crate::node_store::NodeStore) {
        let Some(store) = self.writable_store() else {
            return;
        };
        config_watch::write_guarded(&store.dir().join(crate::node_store::NODE_STORE_FILE), nodes);
    }

    /// Write a profile to `profiles/<file>.styx`, named for the profile —
    /// its own patches only: a song's patches go back to the song
    /// (`songs.styx`, see [`PatchDef::song`]).
    pub fn save_profile(&self, profile: &ProfileDef) {
        let Some(store) = self.writable_store() else {
            return;
        };
        let mut profile = profile.clone();
        // Preset mode's bench is the session's, never the profile's.
        profile
            .patches
            .retain(|p| p.name != crate::profiles::PRESET_BENCH);
        let (song_patches, own): (
            Vec<crate::profiles::PatchDef>,
            Vec<crate::profiles::PatchDef>,
        ) = profile.patches.drain(..).partition(|p| !p.song.is_empty());
        profile.patches = own;
        if !song_patches.is_empty() {
            let songs_path = store.dir().join("songs.styx");
            // Merged into what the file says now — read quietly, so an edit
            // the rig has not loaded yet stays news (and this save, then,
            // leaves the file to it).
            let mut songs = match config_watch::read_quiet::<SongLib>(&songs_path) {
                Read::Ok(l) => l.songs,
                Read::Missing => Vec::new(),
                Read::Bad(_) => {
                    tracing::warn!(
                        "songs.styx does not parse — the song patches were not saved into it"
                    );
                    return self.save_profile_file(&store, profile);
                }
            };
            for song in &mut songs {
                let mine: Vec<crate::profiles::PatchDef> = song_patches
                    .iter()
                    .filter(|p| p.song.eq_ignore_ascii_case(&song.name))
                    .cloned()
                    .collect();
                if !mine.is_empty() {
                    song.patches = mine;
                }
            }
            config_watch::write_guarded(&songs_path, &SongLib { songs });
        }
        self.save_profile_file(&store, profile);
    }

    /// Remove a profile's file. The caller has already decided it may go —
    /// this does not know which profile is active.
    pub fn delete_profile(&self, name: &str) {
        if self.writable_store().is_none() {
            return;
        }
        let path = self.profiles_dir().join(profile_file(name));
        #[cfg(not(target_arch = "wasm32"))]
        if let Some((book, entry)) = signal_rig_host::book::for_path(&path) {
            book.remove(&entry);
            return;
        }
        if let Err(e) = std::fs::remove_file(&path) {
            tracing::warn!("rig library: cannot remove {}: {e}", path.display());
        }
        config_watch::forget(&path);
    }

    pub fn save_drive_presets(&self, presets: &[DrivePresetDef]) {
        let Some(store) = self.writable_store() else {
            return;
        };
        let mut presets = presets.to_vec();
        for dp in &mut presets {
            for option in &mut dp.options {
                store.relativize(&mut option.nam);
            }
        }
        config_watch::write_guarded(
            &store.dir().join("drive-presets.styx"),
            &DrivePresetLib { presets },
        );
    }

    /// Write the songs. A song's patches are kept as the file has them —
    /// they are written by [`save_profile`](Self::save_profile), which holds
    /// the live copies; a song list held elsewhere must not roll them back.
    pub fn save_songs(&self, songs: &[SongDef]) {
        let Some(store) = self.writable_store() else {
            return;
        };
        let path = store.dir().join("songs.styx");
        let on_disk = match config_watch::read_quiet::<SongLib>(&path) {
            Read::Ok(l) => l.songs,
            Read::Missing => Vec::new(),
            // Not overwritten either way (the guard below refuses) — but
            // say why here, where the reason is known.
            Read::Bad(_) => {
                tracing::warn!("songs.styx does not parse — the song list was not saved over it");
                return;
            }
        };
        let songs: Vec<SongDef> = songs
            .iter()
            .map(|s| {
                let mut s = s.clone();
                if let Some(d) = on_disk
                    .iter()
                    .find(|d| d.name.eq_ignore_ascii_case(&s.name))
                {
                    s.patches.clone_from(&d.patches);
                }
                s
            })
            .collect();
        config_watch::write_guarded(&path, &SongLib { songs });
    }

    pub fn save_setlists(&self, setlists: &[SetlistDef]) {
        let Some(store) = self.writable_store() else {
            return;
        };
        config_watch::write_guarded(
            &store.dir().join("setlists.styx"),
            &SetlistLib {
                setlists: setlists.to_vec(),
            },
        );
    }

    /// A guitar photo from the library's `guitars/` folder — a bare file
    /// name only, never a path out of it. Empty when it is not there.
    #[must_use]
    pub fn guitar_photo(&self, file: &str) -> Vec<u8> {
        let name = std::path::Path::new(file);
        if file.is_empty() || name.components().count() != 1 {
            return Vec::new();
        }
        std::fs::read(self.dir.join("guitars").join(name)).unwrap_or_default()
    }

    /// Write `setup.styx` (refused over a file that does not parse).
    pub fn save_setup(&self, setup: &crate::proto::SetupModel) {
        let Some(store) = self.writable_store() else {
            return;
        };
        config_watch::write_guarded(&store.dir().join("setup.styx"), setup);
    }

    /// Write `song-info.styx` (refused over a file that does not parse).
    pub fn save_song_info(&self, info: &SongInfoLib) {
        let Some(store) = self.writable_store() else {
            return;
        };
        config_watch::write_guarded(&store.dir().join("song-info.styx"), info);
    }

    pub fn save_last_state(&self, state: &LastState) {
        let Some(store) = self.writable_store() else {
            return;
        };
        // The rig's own file: written whatever is there.
        config_watch::write_owned(&store.dir().join("last-state.styx"), state);
    }

    /// `None` when the file is missing (fresh install) or unparsable.
    #[must_use]
    pub fn load_last_state(&self) -> Option<LastState> {
        self.store().read("last-state.styx")
    }
}

/// The process's own library ([`RigHome::process_default`]), for callers
/// with no rig of their own — the CLI, the offline tools, a host that plays
/// the player's library. A rig reads and writes through its [`RigHome`].
impl RigLibrary {
    /// [`RigHome::load_or_bootstrap`] on the process's own home.
    #[must_use]
    pub fn load_or_bootstrap() -> Self {
        RigHome::process_default().load_or_bootstrap()
    }

    /// [`RigHome::load_compositions`] on the process's own home.
    #[must_use]
    pub fn load_compositions() -> crate::compose::Compositions {
        RigHome::process_default().load_compositions()
    }

    /// [`RigHome::save_compositions`] on the process's own home.
    pub fn save_compositions(comp: &crate::compose::Compositions) {
        RigHome::process_default().save_compositions(comp);
    }

    /// [`RigHome::save_profile`] on the process's own home.
    pub fn save_profile(profile: &ProfileDef) {
        RigHome::process_default().save_profile(profile);
    }

    /// [`RigHome::save_drive_presets`] on the process's own home.
    pub fn save_drive_presets(presets: &[DrivePresetDef]) {
        RigHome::process_default().save_drive_presets(presets);
    }

    /// Bumped whenever [`RigHome::load_compositions`] reads the files
    /// again (a save, or an edit by hand) — in any rig's library: what a
    /// cache of anything derived from them is keyed by. One count for the
    /// process, so a rig's cache can also move when another rig's library
    /// did; that costs a rebuild, never a stale answer.
    #[must_use]
    pub fn compositions_generation() -> u64 {
        COMPOSITIONS_GENERATION.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl RigHome {
    /// A setup to start from, for this rig's instrument ([`seed_setup_for`]).
    #[must_use]
    pub fn seed_setup(&self) -> crate::proto::SetupModel {
        seed_setup_for(self.instrument)
    }
}

/// A setup to start from: the TMG Strat into the MiniFuse, with the
/// AIRSTEP — and the other guitars, the Scarlett and the MC6 to choose.
#[must_use]
pub fn seed_setup_for(instrument: Instrument) -> crate::proto::SetupModel {
    use crate::proto::{
        AudioRigEntry, ControllerEntry, GuitarEntry, GuitarTone, Pickup, SetupModel, ToneOverride,
    };
    let gates = |floor: f32| vec![floor + 5.0, floor + 10.0, floor + 16.0, floor + 24.0];
    let pickups = |list: &[(&str, &str)]| {
        list.iter()
            .map(|(p, m)| Pickup {
                position: (*p).into(),
                model: (*m).into(),
            })
            .collect()
    };
    let tone =
        |trim_db: f32, floor: f32, noisy: bool, low_cut_hz: f32, bass_db: f32, treble_db: f32| {
            GuitarTone {
                trim_db,
                gates: gates(floor),
                noisy,
                low_cut_hz,
                bass_db,
                mid_db: 0.0,
                treble_db,
            }
        };
    let guitar = |id: &str,
                  name: &str,
                  image: &str,
                  colour: &str,
                  p: Vec<Pickup>,
                  t: GuitarTone,
                  overrides: Vec<ToneOverride>| GuitarEntry {
        id: id.into(),
        name: name.into(),
        image: image.into(),
        colour: colour.into(),
        pickups: p,
        tone: t,
        overrides,
    };
    let rig = |id: &str,
               name: &str,
               device: &str,
               buffer: u32,
               house: &str,
               house_db: f32,
               phones: &str,
               phones_db: f32| AudioRigEntry {
        id: id.into(),
        name: name.into(),
        device: device.into(),
        input: "Input 1".into(),
        rate: 48_000,
        buffer,
        house: house.into(),
        house_db,
        phones: phones.into(),
        phones_db,
        direct_monitor: false,
        target_db: -15.0,
    };
    // The bass's instruments: a Jazz, its neck pickup a Lawler — and the
    // input's low cut down at 25 Hz (a guitar's 70 would take the bass's
    // fundamentals), its gates lower for a held note's long tail.
    let instruments = if instrument == Instrument::Bass {
        vec![
            guitar(
                "jazz",
                "Geddy Lee Jazz Bass",
                "",
                "#18181b",
                pickups(&[
                    ("Bridge", "Fender Jazz single-coil"),
                    ("Neck", "Lawler Jazz"),
                ]),
                tone(0.0, -80.0, false, 25.0, 0.0, 0.0),
                Vec::new(),
            ),
            guitar(
                "pbass",
                "Precision Bass",
                "",
                "#e9e4d8",
                pickups(&[("Middle", "Fender split-coil")]),
                tone(0.0, -80.0, false, 25.0, 0.0, 0.0),
                Vec::new(),
            ),
        ]
    } else {
        Vec::new()
    };
    SetupModel {
        guitars: if !instruments.is_empty() {
            instruments
        } else {
            vec![
                guitar(
                    "strat",
                    "TMG Strat",
                    "strat-white-gold.webp",
                    "#e9e4d8",
                    pickups(&[
                        ("Bridge", "Seymour Duncan SH-PG1b Pearly Gates"),
                        ("Middle", "Lawler Blonde"),
                        ("Neck", "Lawler Blonde"),
                    ]),
                    tone(0.0, -77.0, false, 70.0, 0.0, 0.0),
                    vec![ToneOverride {
                        rig: "stage".into(),
                        parts: vec!["trim".into(), "gates".into()],
                        tone: GuitarTone {
                            trim_db: 2.5,
                            gates: gates(-72.0),
                            ..GuitarTone::default()
                        },
                    }],
                ),
                guitar(
                    "tele",
                    "Blacked Out Tele",
                    "tele-black.webp",
                    "#18181b",
                    pickups(&[("Bridge", "Tele single-coil"), ("Neck", "Tele single-coil")]),
                    tone(3.0, -70.0, true, 80.0, 1.0, -1.5),
                    Vec::new(),
                ),
                guitar(
                    "es339",
                    "Epiphone ES-339",
                    "es339-cherry.webp",
                    "#9f1d2b",
                    pickups(&[("Bridge", "P-90"), ("Neck", "P-90")]),
                    tone(1.5, -68.0, true, 90.0, -1.0, 0.5),
                    vec![ToneOverride {
                        rig: "stage".into(),
                        parts: vec!["noisy".into()],
                        tone: GuitarTone {
                            noisy: false,
                            ..GuitarTone::default()
                        },
                    }],
                ),
                guitar(
                    "goldtop",
                    "Goldtop Les Paul",
                    "lp-goldtop.webp",
                    "#c9a24a",
                    pickups(&[("Bridge", "P-90"), ("Neck", "P-90")]),
                    tone(1.0, -69.0, false, 80.0, -1.5, 1.0),
                    Vec::new(),
                ),
            ]
        },
        guitar_index: 0,
        rigs: vec![
            rig(
                "minifuse",
                "MiniFuse 4",
                "Arturia MiniFuse 4",
                128,
                "Outputs 1–2",
                -6.0,
                "Phones 1",
                -12.0,
            ),
            rig(
                "stage",
                "Scarlett 2i2",
                "Focusrite Scarlett 2i2",
                64,
                "Outputs 1–2",
                0.0,
                "Phones",
                -18.0,
            ),
        ],
        rig_index: 0,
        controllers: vec![
            ControllerEntry {
                id: "airstep".into(),
                name: "AIRSTEP".into(),
                device: "XSonic AIRSTEP".into(),
                channel: 0,
                program_change: true,
                clock: "off".into(),
            },
            ControllerEntry {
                id: "mc6".into(),
                name: "MC6".into(),
                device: "Morningstar MC6".into(),
                channel: 1,
                program_change: true,
                clock: "off".into(),
            },
        ],
        controller_index: 0,
    }
}

/// The library in a book (`signal_rig_host::book`): what the rig reads and
/// writes, end to end through [`RigHome`], once a book stands for its
/// directory.
#[cfg(all(test, not(target_arch = "wasm32")))]
mod book_tests {
    use loro::{ExportMode, VersionVector};
    use signal_rig_host::book::{self, RigBook};

    use super::*;

    /// What `from` has that `to` has not.
    fn delta(from: &RigBook, to: &RigBook) -> Vec<u8> {
        let since: VersionVector = to.doc().oplog_vv();
        from.doc()
            .export(ExportMode::updates(&since))
            .expect("an export")
    }

    #[test]
    fn a_rig_library_moves_into_its_book_and_lives_there() {
        let tmp = tempfile::tempdir().expect("a dir");
        let dir = tmp.path().join("rig");
        let home = RigHome::new(Instrument::Guitar, &dir);

        // A library as it was: styx files, bootstrapped from the defaults.
        let files = home.load_or_bootstrap();
        let names = |lib: &RigLibrary| {
            let mut n: Vec<String> = lib.profiles.iter().map(|p| p.name.clone()).collect();
            n.sort();
            n
        };
        assert!(!files.profiles.is_empty() && !files.setlists.is_empty());

        // A book in its place: the same library, moved in as it is read.
        let book = RigBook::open(&tmp.path().join("Guitar.session"), &dir, false).expect("a book");
        book::register(book.clone());
        let moved = home.load_or_bootstrap();
        assert_eq!(names(&moved), names(&files), "every profile");
        assert_eq!(moved.setlists.len(), files.setlists.len(), "every setlist");
        assert!(book.contains("setlists.styx") && book.contains("songs.styx"));
        assert_eq!(book.names("profiles/").len(), files.profiles.len());

        // A save goes to the book; the old file is the backup.
        let before = std::fs::read(dir.join("setlists.styx")).expect("the old file");
        let mut sets = moved.setlists.clone();
        let mut caa = sets[0].clone();
        caa.name = "CAA 2026".into();
        sets.push(caa);
        home.save_setlists(&sets);
        assert_eq!(
            std::fs::read(dir.join("setlists.styx")).expect("file"),
            before
        );
        assert!(
            home.load_or_bootstrap()
                .setlists
                .iter()
                .any(|s| s.name == "CAA 2026")
        );

        // A profile deleted stays deleted, its old file notwithstanding.
        let gone = files.profiles[0].name.clone();
        home.delete_profile(&gone);
        assert!(
            !home
                .load_or_bootstrap()
                .profiles
                .iter()
                .any(|p| p.name == gone)
        );

        // Another device's edit reaches the rig's hot reload, by name.
        let other = RigBook::open(
            &tmp.path().join("iPad.session"),
            &tmp.path().join("ipad"),
            false,
        )
        .expect("another device");
        other
            .doc()
            .import(&delta(&book, &other))
            .expect("it has the library");
        let mut theirs = sets.clone();
        theirs.retain(|s| s.name != "CAA 2026");
        other
            .write("setlists.styx", &SetlistLib { setlists: theirs })
            .expect("their edit");
        book.doc()
            .import(&delta(&other, &book))
            .expect("it arrives");
        let changes = crate::config_watch::Watcher::new().poll(&dir);
        assert!(
            changes
                .iter()
                .any(|c| c.path == dir.join("setlists.styx") && !c.deleted),
            "the watcher reports it: {changes:?}"
        );
        assert!(
            !home
                .load_or_bootstrap()
                .setlists
                .iter()
                .any(|s| s.name == "CAA 2026")
        );
    }
}

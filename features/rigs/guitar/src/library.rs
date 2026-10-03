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

use std::path::PathBuf;

use facet::Facet;
use signal_rig_host::store::{StyxDir, signal_config_dir};

use crate::config_watch::{self, Read};
use crate::profiles::{
    DrivePresetDef, KeyBindingDef, MidiMapDef, ProfileDef, SetlistDef, SongDef, default_keymap,
    default_midi_map, default_setlists, drive_presets, song_library, worship_def,
};

/// The library directory (`SIGNAL_RIG_DIR` overrides).
#[must_use]
pub fn rig_dir() -> PathBuf {
    if let Ok(p) = std::env::var("SIGNAL_RIG_DIR") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    signal_config_dir().join("rig")
}

/// The styx store over [`rig_dir`].
fn store() -> StyxDir {
    StyxDir::new(rig_dir())
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

/// The store, or `None` when this run does not persist.
///
/// Every save goes through here rather than checking the flag itself: there
/// is then no way to write to the library without having asked whether this
/// run is allowed to, including from a save function nobody has written yet.
fn writable_store() -> Option<StyxDir> {
    if rig_is_ephemeral() {
        tracing::debug!("ephemeral run — skipping library write");
        return None;
    }
    Some(store())
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

/// The profiles the app ships (`profiles/<file>`), seeded into a library
/// that has never had them — see [`seed_profiles`].
const DEFAULT_PROFILES: &[(&str, &str)] = &[
    ("blues.styx", include_str!("../default-config/profiles/blues.styx")),
    ("worship.styx", include_str!("../default-config/profiles/worship.styx")),
];

/// The profile a rig plays when nothing has chosen one (and the one a
/// library that newly gains it switches to, once).
pub const DEFAULT_PROFILE_NAME: &str = "Blues";

/// The shipped Cores frozen into NAM captures (rig-dir-relative
/// `frozen/<name>`, see `crate::freeze`): with them a shipped profile plays
/// without a download.
const DEFAULT_FROZEN: &[(&str, &[u8])] = &[
    ("john-mayer--dumble-L.nam", include_bytes!("../default-config/frozen/john-mayer--dumble-L.nam")),
    ("john-mayer--dumble-klon-L.nam", include_bytes!("../default-config/frozen/john-mayer--dumble-klon-L.nam")),
    ("john-mayer--stereo-L.nam", include_bytes!("../default-config/frozen/john-mayer--stereo-L.nam")),
    ("john-mayer--stereo-stack-L.nam", include_bytes!("../default-config/frozen/john-mayer--stereo-stack-L.nam")),
    ("john-mayer--trio-L.nam", include_bytes!("../default-config/frozen/john-mayer--trio-L.nam")),
    ("john-mayer--two-rock-L.nam", include_bytes!("../default-config/frozen/john-mayer--two-rock-L.nam")),
    ("john-mayer--two-rock-screamer-L.nam", include_bytes!("../default-config/frozen/john-mayer--two-rock-screamer-L.nam")),
    ("john-mayer--vibroverb-L.nam", include_bytes!("../default-config/frozen/john-mayer--vibroverb-L.nam")),
];

/// The NAM captures the default config references (rig-dir-relative
/// `models/<name>`), embedded for first-run seeding.
const DEFAULT_MODELS: &[(&str, &[u8])] = &[
    (
        "VX TB30 BR Edge0 BAL2 CAB FREE.nam",
        include_bytes!("../default-config/models/VX TB30 BR Edge0 BAL2 CAB FREE.nam"),
    ),
    (
        "Fender DRRI _ Clean _ DI Capture (No Cab).nam",
        include_bytes!("../default-config/models/Fender DRRI _ Clean _ DI Capture (No Cab).nam"),
    ),
    (
        "Fender DRRI _ Clean _ SM57 + Royer R-121 + Room _ Full Rig.nam",
        include_bytes!(
            "../default-config/models/Fender DRRI _ Clean _ SM57 + Royer R-121 + Room _ Full Rig.nam"
        ),
    ),
    (
        "Vib Arena Lead LT.nam",
        include_bytes!("../default-config/models/Vib Arena Lead LT.nam"),
    ),
    (
        "Vibrato Verb AA Crunch.nam",
        include_bytes!("../default-config/models/Vibrato Verb AA Crunch.nam"),
    ),
    (
        "Vibrato Verb AA Driven.nam",
        include_bytes!("../default-config/models/Vibrato Verb AA Driven.nam"),
    ),
    (
        "JHS Morning Glory V4 - High Gain Blue.nam",
        include_bytes!("../default-config/models/JHS Morning Glory V4 - High Gain Blue.nam"),
    ),
    (
        "JHS Morning Glory V4 - Low Gain Blue.nam",
        include_bytes!("../default-config/models/JHS Morning Glory V4 - Low Gain Blue.nam"),
    ),
    (
        "JHS Morning Glory V4 - Medium Gain Blue.nam",
        include_bytes!("../default-config/models/JHS Morning Glory V4 - Medium Gain Blue.nam"),
    ),
    (
        "King of Tone both sides.nam",
        include_bytes!("../default-config/models/King of Tone both sides.nam"),
    ),
    (
        "King of Tone ver4 Red channel set to Boost.nam",
        include_bytes!("../default-config/models/King of Tone ver4 Red channel set to Boost.nam"),
    ),
    // The Blues profile's pedals and amps (TONE3000 captures).
    (
        "KLON 2.nam",
        include_bytes!("../default-config/models/KLON 2.nam"),
    ),
    (
        "marshall-bluesbreaker-pedal-setting1.nam",
        include_bytes!("../default-config/models/marshall-bluesbreaker-pedal-setting1.nam"),
    ),
    (
        "46-TS808_Hot_Lvl6_OD1_T5.nam",
        include_bytes!("../default-config/models/46-TS808_Hot_Lvl6_OD1_T5.nam"),
    ),
    (
        "Dumble Steel SS Clean.nam",
        include_bytes!("../default-config/models/Dumble Steel SS Clean.nam"),
    ),
    (
        "Dumble Steel SS Drive 1.nam",
        include_bytes!("../default-config/models/Dumble Steel SS Drive 1.nam"),
    ),
    (
        "Two-Rock_ John Mayer Signature Prototype.nam",
        include_bytes!("../default-config/models/Two-Rock_ John Mayer Signature Prototype.nam"),
    ),
    (
        "Fender_ Vibroverb_ 1964.nam",
        include_bytes!("../default-config/models/Fender_ Vibroverb_ 1964.nam"),
    ),
    (
        "Fender_ Vibroverb_ 1964  - Dumble_ Steel String Singer_ _002 -  Two-Rock_ John Mayer Signature Prototype __Signature _83__.nam",
        include_bytes!("../default-config/models/Fender_ Vibroverb_ 1964  - Dumble_ Steel String Singer_ _002 -  Two-Rock_ John Mayer Signature Prototype __Signature _83__.nam"),
    ),
];

/// Write any default NAM model missing from `<rig_dir>/models/`.
fn seed_models() {
    seed_files(&rig_dir().join("models"), DEFAULT_MODELS);
    seed_files(&rig_dir().join("frozen"), DEFAULT_FROZEN);
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

/// The store over [`PROFILES_DIR`]. NAM paths still resolve against the rig
/// directory (`models/…`), so resolving goes through [`store`], not this.
fn profiles_store() -> StyxDir {
    StyxDir::new(rig_dir().join(PROFILES_DIR))
}

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

/// The profiles directory.
#[must_use]
pub fn profiles_dir() -> PathBuf {
    rig_dir().join(PROFILES_DIR)
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

/// Resolve a profile's rig-dir-relative capture paths, as a load does.
pub(crate) fn resolve_profile(profile: &mut ProfileDef) {
    let store = store();
    for preset in &mut profile.presets {
        store.resolve(&mut preset.nam);
    }
}

/// Resolve the drive presets' capture paths, as a load does.
pub(crate) fn resolve_drive_presets(presets: &mut [DrivePresetDef]) {
    let store = store();
    for dp in presets {
        for option in &mut dp.options {
            store.resolve(&mut option.nam);
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

/// Every profile in `profiles/`, sorted by name.
///
/// A rig from before there were several profiles has one `profile.styx`
/// instead. That file becomes the first profile: written into `profiles/`
/// and renamed `profile.styx.migrated`, so a hand edit to the old file is
/// not silently ignored — it is plainly not the file any more. A run that
/// may not write (design, ephemeral) reads it in place and leaves it alone.
fn load_profiles(store: &StyxDir) -> Vec<ProfileDef> {
    let dir = profiles_store();
    let mut files: Vec<String> = std::fs::read_dir(dir.dir())
        .map(|rd| {
            rd.filter_map(Result::ok)
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|n| n.ends_with(".styx"))
                .collect()
        })
        .unwrap_or_default();
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
        let legacy = read_or_seed(store, "profile.styx", DEFAULT_PROFILE, worship_def);
        if writable_store().is_some() {
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

/// The shipped profiles a library has been given, by name, one a line in
/// `profiles/.seeded`: a shipped profile is seeded once, so one the player
/// deletes stays deleted.
const SEEDED_MARKER: &str = ".seeded";

/// The default profile a library has been switched to, in
/// `profiles/.default`: a library opens on [`DEFAULT_PROFILE_NAME`] once
/// when it changes (an install from before it, whose last-played profile
/// would otherwise win), then remembers the player's choice.
const DEFAULT_MARKER: &str = ".default";

/// Add each shipped profile (`DEFAULT_PROFILES`) the library has never had
/// — a fresh install, or one from before it shipped — to `profiles` and to
/// `profiles/`. Whether the library should open on [`DEFAULT_PROFILE_NAME`]
/// this once: it was just seeded, or the library has not opened on it yet.
fn seed_profiles(profiles: &mut Vec<ProfileDef>) -> bool {
    let dir = profiles_store();
    let marker = dir.dir().join(SEEDED_MARKER);
    let mut seeded: Vec<String> = std::fs::read_to_string(&marker)
        .map(|t| t.lines().map(str::to_string).collect())
        .unwrap_or_default();
    let mut gained_default = false;
    let mut changed = false;
    for (file, text) in DEFAULT_PROFILES {
        let def: ProfileDef = match facet_styx::from_str(text) {
            Ok(def) => def,
            Err(e) => {
                tracing::warn!(file, error = %e, "rig library: shipped profile does not parse");
                continue;
            }
        };
        let had = seeded.iter().any(|n| n.eq_ignore_ascii_case(&def.name));
        let present = profiles.iter().any(|p| p.name.eq_ignore_ascii_case(&def.name));
        if !had {
            seeded.push(def.name.clone());
            changed = true;
        }
        if had || present {
            continue;
        }
        if writable_store().is_some() {
            config_watch::write_guarded(&dir.dir().join(file), &def);
        }
        tracing::info!(profile = %def.name, "rig library: seeded shipped profile");
        gained_default |= def.name.eq_ignore_ascii_case(DEFAULT_PROFILE_NAME);
        profiles.push(def);
    }
    if changed && writable_store().is_some() {
        let _ = std::fs::create_dir_all(dir.dir());
        if let Err(e) = std::fs::write(&marker, seeded.join("\n") + "\n") {
            tracing::warn!("rig library: cannot write {}: {e}", marker.display());
        }
    }
    let default_marker = dir.dir().join(DEFAULT_MARKER);
    let opened_on = std::fs::read_to_string(&default_marker).unwrap_or_default();
    if !opened_on.trim().eq_ignore_ascii_case(DEFAULT_PROFILE_NAME) && writable_store().is_some() {
        gained_default = true;
        let _ = std::fs::create_dir_all(dir.dir());
        if let Err(e) = std::fs::write(&default_marker, format!("{DEFAULT_PROFILE_NAME}\n")) {
            tracing::warn!("rig library: cannot write {}: {e}", default_marker.display());
        }
    }
    profiles.sort_by_key(|p| p.name.to_lowercase());
    gained_default
}

/// The shipped drive pedals a library has been given, by name, one a line
/// (`.seeded-drive-presets`): seeded once, so one the player deletes stays
/// deleted.
const SEEDED_DRIVES_MARKER: &str = ".seeded-drive-presets";

/// Add each shipped drive pedal (`DEFAULT_DRIVE_PRESETS`) a library from
/// before it shipped has never had — a shipped profile names its pedals,
/// and a slot whose pedal the library lacks plays nothing ("Empty"). Whether
/// any was added (the caller saves).
fn seed_drive_presets(presets: &mut Vec<DrivePresetDef>) -> bool {
    let Some(store) = writable_store() else {
        return false;
    };
    let Ok(shipped) = facet_styx::from_str::<DrivePresetLib>(DEFAULT_DRIVE_PRESETS) else {
        tracing::warn!("rig library: shipped drive presets do not parse");
        return false;
    };
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
        if !presets.iter().any(|p| p.name.eq_ignore_ascii_case(&dp.name)) {
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

/// Add each shipped module preset and block preset a library from before
/// it shipped has never had, to `modules.styx` and `blocks.styx` — once
/// (`.seeded-modules`, `.seeded-blocks`), so one the player deletes stays
/// deleted. A file the library does not have yet is seeded whole on first
/// read (see `read_compositions`); this is for the one it already has.
fn seed_compositions() {
    let Some(store) = writable_store() else { return };
    seed_entries::<crate::compose::ModuleLib, crate::compose::ModulePresetDef>(
        &store,
        crate::compose::MODULES_FILE,
        DEFAULT_MODULES,
        ".seeded-modules",
        |l| &mut l.presets,
        |p| format!("{}\t{}", p.module, p.name),
    );
    seed_entries::<crate::compose::BlockLib, crate::compose::BlockPresetDef>(
        &store,
        crate::compose::BLOCKS_FILE,
        DEFAULT_BLOCKS,
        ".seeded-blocks",
        |l| &mut l.presets,
        |p| format!("{}\t{}", p.block_type, p.name),
    );
}

/// [`seed_compositions`] for one file: the shipped entries (by `key`) not in
/// the library's file nor in its `marker` are added and written back.
fn seed_entries<L, E>(
    store: &StyxDir,
    file: &str,
    shipped: &str,
    marker: &str,
    list: impl Fn(&mut L) -> &mut Vec<E>,
    key: impl Fn(&E) -> String,
) where
    L: for<'a> Facet<'a>,
    E: Clone,
{
    let path = store.dir().join(file);
    let Read::Ok(mut lib) = config_watch::read_tracked::<L>(&path) else { return };
    let Ok(mut ship) = facet_styx::from_str::<L>(shipped) else {
        tracing::warn!(file, "rig library: shipped compositions do not parse");
        return;
    };
    let marker = store.dir().join(marker);
    let mut seeded: Vec<String> = std::fs::read_to_string(&marker)
        .map(|t| t.lines().map(str::to_string).collect())
        .unwrap_or_default();
    let have: Vec<String> = list(&mut lib).iter().map(&key).map(|k| k.to_lowercase()).collect();
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
    if added > 0 {
        config_watch::write_guarded(&path, &lib);
        tracing::info!(file, added, "rig library: seeded shipped presets");
    }
    if let Err(e) = std::fs::write(&marker, seeded.join("\n") + "\n") {
        tracing::warn!("rig library: cannot write {}: {e}", marker.display());
    }
}

/// Write `profile` (its own patches only) to `profiles/<file>.styx`.
fn save_profile_file(store: &StyxDir, mut profile: ProfileDef) {
    for preset in &mut profile.presets {
        store.relativize(&mut preset.nam);
    }
    config_watch::write_guarded(
        &profiles_store().dir().join(profile_file(&profile.name)),
        &profile,
    );
}

impl RigLibrary {
    /// Load the library, bootstrapping any missing file (and the NAM
    /// models the defaults reference) from the embedded in-repo default
    /// config, so the directory is always complete.
    pub fn load_or_bootstrap() -> Self {
        seed_models();
        let store = store();
        let mut profiles = load_profiles(&store);
        let gained_default = seed_profiles(&mut profiles);
        Self::split_core_once(&store, &mut profiles);
        let mut drive_presets =
            read_or_seed::<DrivePresetLib>(&store, "drive-presets.styx", DEFAULT_DRIVE_PRESETS, || {
                DrivePresetLib {
                    presets: drive_presets(),
                }
            })
            .presets;
        if seed_drive_presets(&mut drive_presets) {
            Self::save_drive_presets(&drive_presets);
        }
        seed_compositions();
        let songs = read_or_seed::<SongLib>(&store, "songs.styx", DEFAULT_SONGS, || SongLib {
            songs: song_library(),
        })
        .songs;
        let setlists =
            read_or_seed::<SetlistLib>(&store, "setlists.styx", DEFAULT_SETLISTS, || SetlistLib {
                setlists: default_setlists(),
            })
            .setlists;
        let midi_map =
            read_or_seed::<MidiMapDef>(&store, "midi.styx", DEFAULT_MIDI, default_midi_map);
        let keymap =
            read_or_seed::<KeymapLib>(&store, "keymap.styx", DEFAULT_KEYMAP, || KeymapLib {
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
        let wanted = Self::load_last_state()
            .map(|s| s.profile)
            .filter(|p| !p.is_empty() && !gained_default)
            .unwrap_or_else(|| DEFAULT_PROFILE_NAME.to_string());
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
        Self {
            profile,
            profiles,
            drive_presets,
            songs,
            setlists,
            midi_map,
            keymap,
        }
    }

    /// The module-preset and preset libraries every profile composes from
    /// (`modules.styx`, `presets.styx`). Missing reads as empty — a rig with
    /// no compositions yet plays its profiles exactly as written.
    #[must_use]
    pub fn load_compositions() -> crate::compose::Compositions {
        // Cached by the three files' modification stamps: the macro bar
        // resolves its responses on every patch switch, and a switch's
        // follow-up must not parse three styx files to find nothing changed.
        // A save (here or by hand) moves a stamp and the next call re-reads.
        type Stamp = Vec<Option<(std::time::SystemTime, u64)>>;
        static CACHE: std::sync::Mutex<
            Option<(std::path::PathBuf, Stamp, crate::compose::Compositions)>,
        > = std::sync::Mutex::new(None);
        let dir = rig_dir();
        let stamp: Stamp = [
            crate::compose::MODULES_FILE,
            crate::compose::PRESETS_FILE,
            crate::compose::BLOCKS_FILE,
        ]
        .iter()
        .map(|f| {
            std::fs::metadata(dir.join(f))
                .ok()
                .and_then(|m| Some((m.modified().ok()?, m.len())))
        })
        .collect();
        if let Ok(cache) = CACHE.lock() {
            if let Some((d, st, comp)) = cache.as_ref() {
                if *d == dir && *st == stamp {
                    return comp.clone();
                }
            }
        }
        let last_good = CACHE.lock().ok().and_then(|c| {
            c.as_ref()
                .filter(|(d, _, _)| *d == dir)
                .map(|(_, _, comp)| comp.clone())
        });
        let comp = Self::read_compositions(last_good.as_ref());
        if let Ok(mut cache) = CACHE.lock() {
            *cache = Some((dir, stamp, comp.clone()));
        }
        comp
    }

    /// Read the three composition files. One that does not parse keeps what
    /// was last read from it (`last_good`) rather than reading as empty — an
    /// empty module library would rebuild every patch without its amp.
    fn read_compositions(
        last_good: Option<&crate::compose::Compositions>,
    ) -> crate::compose::Compositions {
        fn one<T: for<'a> Facet<'a> + Default>(file: &str, seed: &str) -> Result<T, ()> {
            let path = rig_dir().join(file);
            match config_watch::read_tracked::<T>(&path) {
                Read::Ok(v) => Ok(v),
                // A library that has never had it gets the shipped one:
                // without it a shipped profile's composed patches resolve
                // to nothing (written where this run may write).
                Read::Missing => {
                    if writable_store().is_some() {
                        let _ = std::fs::create_dir_all(rig_dir());
                        if let Err(e) = std::fs::write(&path, seed) {
                            tracing::warn!("rig library: seed {file} failed: {e}");
                        }
                    }
                    Ok(facet_styx::from_str(seed).unwrap_or_default())
                }
                Read::Bad(e) => {
                    report_bad(&path, &e);
                    Err(())
                }
            }
        }
        let store = store();
        let modules =
            one::<crate::compose::ModuleLib>(crate::compose::MODULES_FILE, DEFAULT_MODULES).map(|l| l.presets);
        let fresh_modules = modules.is_ok();
        let mut modules =
            modules.unwrap_or_else(|()| last_good.map(|c| c.modules.clone()).unwrap_or_default());
        if fresh_modules {
            for m in &mut modules {
                for snap in &mut m.snapshots {
                    store.resolve(&mut snap.nam);
                    store.resolve(&mut snap.nam2);
                }
            }
        }
        let presets = one::<crate::compose::PresetLib>(crate::compose::PRESETS_FILE, DEFAULT_PRESETS)
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
        let blocks = one::<crate::compose::BlockLib>(crate::compose::BLOCKS_FILE, DEFAULT_BLOCKS)
            .map(|l| l.presets)
            .unwrap_or_else(|()| last_good.map(|c| c.blocks.clone()).unwrap_or_default());
        crate::compose::Compositions {
            modules,
            presets,
            blocks,
        }
    }

    /// Presets become the Core module, once: a library whose presets still
    /// hold time effects, pre effects or chorus and tremolo settings has
    /// them moved out ([`crate::compose::split_core`] — every patch keeps
    /// its sound), the old files kept beside the new as `*.styx.migrated`.
    /// A run that may not write leaves the library as it is (it still
    /// plays: the split changes where settings live, not what plays).
    fn split_core_once(store: &StyxDir, profiles: &mut Vec<ProfileDef>) {
        if writable_store().is_none() {
            return;
        }
        let comp = Self::load_compositions();
        let (split, split_profiles) = crate::compose::split_core(&comp, profiles);
        let changed = format!("{:?}", split.presets) != format!("{:?}", comp.presets);
        if !changed {
            return;
        }
        let keep = |path: std::path::PathBuf| {
            if path.exists() {
                if let Err(e) = std::fs::copy(&path, path.with_extension("styx.migrated")) {
                    tracing::warn!("rig library: cannot keep {} before the Core split: {e}", path.display());
                }
            }
        };
        for f in [crate::compose::PRESETS_FILE, crate::compose::MODULES_FILE, crate::compose::BLOCKS_FILE] {
            keep(store.dir().join(f));
        }
        for p in split_profiles.iter() {
            keep(profiles_store().dir().join(profile_file(&p.name)));
        }
        Self::save_compositions(&split);
        for p in &split_profiles {
            save_profile_file(store, p.clone());
        }
        tracing::info!("rig library: presets are the Core module now — time effects, pre effects, chorus and tremolo moved out (old files kept as *.styx.migrated)");
        *profiles = split_profiles;
    }

    /// Write both composition libraries back.
    pub fn save_compositions(comp: &crate::compose::Compositions) {
        let Some(store) = writable_store() else {
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
    pub fn load_node_store() -> crate::node_store::NodeStore {
        // Read per use, so it is always what the rig holds: tracked, so the
        // save that follows a read is not mistaken for an overwrite.
        match config_watch::read_tracked(&rig_dir().join(crate::node_store::NODE_STORE_FILE)) {
            Read::Ok(v) => v,
            Read::Missing => crate::node_store::NodeStore::default(),
            Read::Bad(e) => {
                report_bad(&rig_dir().join(crate::node_store::NODE_STORE_FILE), &e);
                crate::node_store::NodeStore::default()
            }
        }
    }

    /// Write it back. Best-effort, like the profile: losing a preset is not
    /// worth failing a rig for.
    pub fn save_node_store(nodes: &crate::node_store::NodeStore) {
        let Some(store) = writable_store() else {
            return;
        };
        config_watch::write_guarded(&store.dir().join(crate::node_store::NODE_STORE_FILE), nodes);
    }

    /// Write a profile to `profiles/<file>.styx`, named for the profile —
    /// its own patches only: a song's patches go back to the song
    /// (`songs.styx`, see [`PatchDef::song`]).
    pub fn save_profile(profile: &ProfileDef) {
        let Some(store) = writable_store() else {
            return;
        };
        let mut profile = profile.clone();
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
                    return save_profile_file(&store, profile);
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
        save_profile_file(&store, profile);
    }

    /// Remove a profile's file. The caller has already decided it may go —
    /// this does not know which profile is active.
    pub fn delete_profile(name: &str) {
        if writable_store().is_none() {
            return;
        }
        let path = profiles_store().dir().join(profile_file(name));
        if let Err(e) = std::fs::remove_file(&path) {
            tracing::warn!("rig library: cannot remove {}: {e}", path.display());
        }
        config_watch::forget(&path);
    }

    pub fn save_drive_presets(presets: &[DrivePresetDef]) {
        let Some(store) = writable_store() else {
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
    pub fn save_songs(songs: &[SongDef]) {
        let Some(store) = writable_store() else {
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

    pub fn save_setlists(setlists: &[SetlistDef]) {
        let Some(store) = writable_store() else {
            return;
        };
        config_watch::write_guarded(
            &store.dir().join("setlists.styx"),
            &SetlistLib {
                setlists: setlists.to_vec(),
            },
        );
    }

    pub fn save_last_state(state: &LastState) {
        let Some(store) = writable_store() else {
            return;
        };
        // The rig's own file: written whatever is there.
        config_watch::write_owned(&store.dir().join("last-state.styx"), state);
    }

    /// `None` when the file is missing (fresh install) or unparsable.
    #[must_use]
    pub fn load_last_state() -> Option<LastState> {
        store().read("last-state.styx")
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_shipped_compositions_parse_with_the_time_delay_and_reverb_presets() {
        let modules: crate::compose::ModuleLib = facet_styx::from_str(super::DEFAULT_MODULES).expect("modules.styx parses");
        let blocks: crate::compose::BlockLib = facet_styx::from_str(super::DEFAULT_BLOCKS).expect("blocks.styx parses");
        let count = |m: &str| modules.presets.iter().filter(|p| p.module == m).count();
        assert!(count("Time") >= 10, "Time presets ship");
        assert!(count("Delay") >= 9, "Delay presets ship");
        assert!(count("Reverb") >= 8, "Reverb presets ship");
        assert!(blocks.presets.len() >= 100, "block presets ship");
    }

    #[test]
    fn nam_paths_roundtrip_relative() {
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("SIGNAL_RIG_DIR", "/tmp/fts-test-rig") };
        let store = super::store();
        let mut p = String::from("models/x.nam");
        store.resolve(&mut p);
        assert_eq!(p, "/tmp/fts-test-rig/models/x.nam");
        store.relativize(&mut p);
        assert_eq!(p, "models/x.nam");
        let mut abs = String::from("/elsewhere/y.nam");
        store.resolve(&mut abs);
        store.relativize(&mut abs);
        assert_eq!(abs, "/elsewhere/y.nam");
    }

    /// The shipped config and the structs that read it must agree — a
    /// mismatch does not fail a build, it fails a first run, on whatever
    /// machine the binary was installed on.
    #[test]
    fn the_shipped_profile_parses_and_carries_its_provenance() {
        let profile: super::ProfileDef =
            facet_styx::from_str(super::DEFAULT_PROFILE).expect("default profile.styx parses");

        let ac30 = profile
            .presets
            .iter()
            .find(|p| p.name == "AC30 Clean")
            .expect("the worship rig has an AC30");

        // The hash is what lets the preset find its own creator, licence and
        // cover art in the NAM catalog. Without it the preset still plays and
        // simply shows nothing — which is exactly why a silent typo here
        // would go unnoticed.
        assert_eq!(
            ac30.hash, "af01655f210266635156342a12382d94e5099159a645874db0abf848d790ec6b",
            "AC30 preset lost the content hash of its capture"
        );
        assert!(
            ac30.nam.starts_with("models/"),
            "shipped captures are rig-dir-relative so the config is portable, got {}",
            ac30.nam
        );
    }

    #[test]
    fn last_state_roundtrips() {
        // Same dir as the sibling test — tests share the process env.
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("SIGNAL_RIG_DIR", "/tmp/fts-test-rig") };
        let state = super::LastState {
            setlist_index: 2,
            song_index: 5,
            part_index: 1,
            active_patch: "Lead Big".to_string(),
            tempo_bpm: 74.0,
            profile: "Blues".to_string(),
            perform_mode: 2,
            master_trim_db: -4.5,
            phones_volume: 0.6,
            phones_guitar: 0.8,
            phones_mix: 0.7,
            main_mute: true,
        };
        super::RigLibrary::save_last_state(&state);
        let back = super::RigLibrary::load_last_state().expect("last-state.styx roundtrip");
        assert_eq!(back.setlist_index, 2);
        assert_eq!(back.perform_mode, 2, "setlist mode survives a restart");
        assert_eq!(back.song_index, 5);
        assert_eq!(back.part_index, 1);
        assert_eq!(back.active_patch, "Lead Big");
        assert_eq!(back.tempo_bpm, 74.0);
        assert_eq!(
            back.master_trim_db, -4.5,
            "the main fader survives a restart"
        );
        // A last-state from before the fader was kept opens at −6 dB.
        let old: super::LastState =
            facet_styx::from_str("setlist_index 1\nperform_mode 2\n").expect("old state parses");
        assert_eq!(old.master_trim_db, -6.0);
        assert_eq!(back.profile, "Blues");
        // The phones come back as they were; an old file opens at unity.
        assert_eq!(
            (back.phones_volume, back.phones_guitar, back.phones_mix),
            (0.6, 0.8, 0.7)
        );
        assert!(back.main_mute);
        assert_eq!(
            (old.phones_volume, old.phones_guitar, old.phones_mix),
            (0.75, 0.75, 0.75)
        );
        assert!(!old.main_mute);
    }

    #[test]
    fn a_profile_file_is_its_name_and_stays_in_the_directory() {
        assert_eq!(super::profile_file("Worship"), "worship.styx");
        assert_eq!(super::profile_file("Rock (Live)"), "rock-live.styx");
        assert_eq!(super::profile_file("../../etc"), "etc.styx");
        assert_eq!(super::profile_file("  "), "profile.styx");
    }
}

#[cfg(test)]
mod song_patch_tests {
    use super::*;

    fn patch(name: &str) -> crate::profiles::PatchDef {
        let mut p = crate::profiles::worship_def().patches[0].clone();
        p.name = name.to_string();
        p
    }

    /// A song's patches join the profile it plays on, tagged with the song;
    /// a song on another profile keeps its patches to itself; a name the
    /// profile already has is not duplicated.
    #[test]
    fn song_patches_join_their_profile_tagged() {
        let mut profile = crate::profiles::worship_def();
        let own = profile.patches.len();
        let clash = profile.patches[0].name.clone();
        let mut washed = crate::profiles::song_library().remove(0);
        washed.name = "WASHED".into();
        washed.profile = String::new();
        washed.patches = vec![patch("Dry Chorus Clean"), patch(&clash)];
        let mut other = washed.clone();
        other.name = "Elsewhere".into();
        other.profile = "Metal".into();
        other.patches = vec![patch("Chug")];
        attach_song_patches(&mut profile, &[washed, other]);
        assert_eq!(profile.patches.len(), own + 1);
        let added = profile.patches.last().unwrap();
        assert_eq!(
            (added.name.as_str(), added.song.as_str()),
            ("Dry Chorus Clean", "WASHED")
        );
        assert!(
            profile.patches[..own].iter().all(|p| p.song.is_empty()),
            "the profile's own stay its own"
        );
    }
}

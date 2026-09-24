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
    env_flag("SIGNAL_RIG_EPHEMERAL") || rig_is_design()
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
];

/// Write any default NAM model missing from `<rig_dir>/models/`.
fn seed_models() {
    let dir = rig_dir().join("models");
    for (name, bytes) in DEFAULT_MODELS {
        let path = dir.join(name);
        if path.exists() {
            continue;
        }
        if let Err(e) = std::fs::create_dir_all(&dir) {
            tracing::warn!("rig library: cannot create {}: {e}", dir.display());
            return;
        }
        if let Err(e) = std::fs::write(&path, bytes) {
            tracing::warn!("rig library: seed model {name} failed: {e}");
        } else {
            tracing::info!("rig library: seeded model {name}");
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

/// Put every song's patches on `profile` (tagged with their song), for the
/// songs played on it — those naming it, and those naming no profile (they
/// play on whatever is loaded).
fn attach_song_patches(profile: &mut ProfileDef, songs: &[SongDef]) {
    for song in songs {
        if !song.profile.is_empty() && !song.profile.eq_ignore_ascii_case(&profile.name) {
            continue;
        }
        for p in &song.patches {
            if profile.patches.iter().any(|x| x.name.eq_ignore_ascii_case(&p.name)) {
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
        .filter_map(|f| dir.read::<ProfileDef>(f))
        .collect();
    if profiles.is_empty() {
        let legacy = store.read_or_seed::<ProfileDef>("profile.styx", DEFAULT_PROFILE, worship_def);
        if writable_store().is_some() {
            dir.write(&profile_file(&legacy.name), &legacy);
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

impl RigLibrary {
    /// Load the library, bootstrapping any missing file (and the NAM
    /// models the defaults reference) from the embedded in-repo default
    /// config, so the directory is always complete.
    pub fn load_or_bootstrap() -> Self {
        seed_models();
        let store = store();
        let mut profiles = load_profiles(&store);
        let mut drive_presets = store
            .read_or_seed::<DrivePresetLib>("drive-presets.styx", DEFAULT_DRIVE_PRESETS, || {
                DrivePresetLib {
                    presets: drive_presets(),
                }
            })
            .presets;
        let songs = store
            .read_or_seed::<SongLib>("songs.styx", DEFAULT_SONGS, || SongLib {
                songs: song_library(),
            })
            .songs;
        let setlists = store
            .read_or_seed::<SetlistLib>("setlists.styx", DEFAULT_SETLISTS, || SetlistLib {
                setlists: default_setlists(),
            })
            .setlists;
        let midi_map =
            store.read_or_seed::<MidiMapDef>("midi.styx", DEFAULT_MIDI, default_midi_map);
        let keymap = store
            .read_or_seed::<KeymapLib>("keymap.styx", DEFAULT_KEYMAP, || KeymapLib {
                bindings: default_keymap(),
            })
            .bindings;
        for profile in &mut profiles {
            for preset in &mut profile.presets {
                store.resolve(&mut preset.nam);
            }
            attach_song_patches(profile, &songs);
        }
        let wanted = Self::load_last_state()
            .map(|s| s.profile)
            .unwrap_or_default();
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
        let store = store();
        let mut modules = store
            .read::<crate::compose::ModuleLib>(crate::compose::MODULES_FILE)
            .unwrap_or_default()
            .presets;
        for m in &mut modules {
            for snap in &mut m.snapshots {
                store.resolve(&mut snap.nam);
                store.resolve(&mut snap.nam2);
            }
        }
        let presets = store
            .read::<crate::compose::PresetLib>(crate::compose::PRESETS_FILE)
            .unwrap_or_default()
            .presets;
        let blocks = store
            .read::<crate::compose::BlockLib>(crate::compose::BLOCKS_FILE)
            .unwrap_or_default()
            .presets;
        crate::compose::Compositions {
            modules,
            presets,
            blocks,
        }
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
        store.write(
            crate::compose::MODULES_FILE,
            &crate::compose::ModuleLib { presets: modules },
        );
        store.write(
            crate::compose::PRESETS_FILE,
            &crate::compose::PresetLib {
                presets: comp.presets.clone(),
            },
        );
        store.write(
            crate::compose::BLOCKS_FILE,
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
        store()
            .read(crate::node_store::NODE_STORE_FILE)
            .unwrap_or_default()
    }

    /// Write it back. Best-effort, like the profile: losing a preset is not
    /// worth failing a rig for.
    pub fn save_node_store(nodes: &crate::node_store::NodeStore) {
        let Some(store) = writable_store() else {
            return;
        };
        store.write(crate::node_store::NODE_STORE_FILE, nodes);
    }

    /// Write a profile to `profiles/<file>.styx`, named for the profile —
    /// its own patches only: a song's patches go back to the song
    /// (`songs.styx`, see [`PatchDef::song`]).
    pub fn save_profile(profile: &ProfileDef) {
        let Some(store) = writable_store() else {
            return;
        };
        let mut profile = profile.clone();
        let (song_patches, own): (Vec<crate::profiles::PatchDef>, Vec<crate::profiles::PatchDef>) =
            profile.patches.drain(..).partition(|p| !p.song.is_empty());
        profile.patches = own;
        if !song_patches.is_empty() {
            let mut songs = store
                .read::<SongLib>("songs.styx")
                .map(|l| l.songs)
                .unwrap_or_default();
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
            store.write("songs.styx", &SongLib { songs });
        }
        for preset in &mut profile.presets {
            store.relativize(&mut preset.nam);
        }
        profiles_store().write(&profile_file(&profile.name), &profile);
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
        store.write("drive-presets.styx", &DrivePresetLib { presets });
    }

    /// Write the songs. A song's patches are kept as the file has them —
    /// they are written by [`save_profile`](Self::save_profile), which holds
    /// the live copies; a song list held elsewhere must not roll them back.
    pub fn save_songs(songs: &[SongDef]) {
        let Some(store) = writable_store() else {
            return;
        };
        let on_disk = store
            .read::<SongLib>("songs.styx")
            .map(|l| l.songs)
            .unwrap_or_default();
        let songs: Vec<SongDef> = songs
            .iter()
            .map(|s| {
                let mut s = s.clone();
                if let Some(d) = on_disk.iter().find(|d| d.name.eq_ignore_ascii_case(&s.name)) {
                    s.patches.clone_from(&d.patches);
                }
                s
            })
            .collect();
        store.write("songs.styx", &SongLib { songs });
    }

    pub fn save_setlists(setlists: &[SetlistDef]) {
        let Some(store) = writable_store() else {
            return;
        };
        store.write(
            "setlists.styx",
            &SetlistLib {
                setlists: setlists.to_vec(),
            },
        );
    }

    pub fn save_last_state(state: &LastState) {
        let Some(store) = writable_store() else {
            return;
        };
        store.write("last-state.styx", state);
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
        };
        super::RigLibrary::save_last_state(&state);
        let back = super::RigLibrary::load_last_state().expect("last-state.styx roundtrip");
        assert_eq!(back.setlist_index, 2);
        assert_eq!(back.song_index, 5);
        assert_eq!(back.part_index, 1);
        assert_eq!(back.active_patch, "Lead Big");
        assert_eq!(back.tempo_bpm, 74.0);
        assert_eq!(back.profile, "Blues");
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
        assert_eq!((added.name.as_str(), added.song.as_str()), ("Dry Chorus Clean", "WASHED"));
        assert!(profile.patches[..own].iter().all(|p| p.song.is_empty()), "the profile's own stay its own");
    }
}

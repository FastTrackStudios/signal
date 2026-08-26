//! The runtime rig library — everything the rig plays from, as plain styx
//! text files in one directory. Portable, git-trackable, and directly
//! editable: an LLM (or a human in a text editor) can create presets, add
//! patches, write patch-level overrides, rename things — then hit reload.
//!
//! ```text
//! <config>/signal/rig/          (override: SIGNAL_RIG_DIR)
//!   profile.styx        ProfileDef — presets pool, patches (+overrides),
//!                       stacks, drive-slot assignments
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

use std::path::PathBuf;

use facet::Facet;
use signal_rig_host::store::{signal_config_dir, StyxDir};

use crate::profiles::{
    default_keymap, default_midi_map, default_setlists, drive_presets, song_library, worship_def,
    DrivePresetDef, KeyBindingDef, MidiMapDef, ProfileDef, SetlistDef, SongDef,
};

/// The library directory (`SIGNAL_RIG_DIR` overrides).
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
/// meter pump on patch/song/part/setlist/tempo changes and restored on the
/// next open, so a crash restart mid-set lands back on the same song
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
}

/// Everything loaded from the rig directory.
#[derive(Clone, Debug)]
pub struct RigLibrary {
    pub profile: ProfileDef,
    pub drive_presets: Vec<DrivePresetDef>,
    pub songs: Vec<SongDef>,
    pub setlists: Vec<SetlistDef>,
    pub midi_map: MidiMapDef,
    pub keymap: Vec<KeyBindingDef>,
}

// The in-repo default config, embedded so installed binaries can seed a
// fresh machine without a checkout.
const DEFAULT_PROFILE: &str = include_str!("../default-config/profile.styx");
const DEFAULT_DRIVE_PRESETS: &str = include_str!("../default-config/drive-presets.styx");
const DEFAULT_SONGS: &str = include_str!("../default-config/songs.styx");
const DEFAULT_SETLISTS: &str = include_str!("../default-config/setlists.styx");
const DEFAULT_MIDI: &str = include_str!("../default-config/midi.styx");
const DEFAULT_KEYMAP: &str = include_str!("../default-config/keymap.styx");

/// The NAM captures the default config references (rig-dir-relative
/// `models/<name>`), embedded for first-run seeding.
const DEFAULT_MODELS: &[(&str, &[u8])] = &[
    (
        "'65 AC30_6 - The Iconic Cleanish.nam",
        include_bytes!("../default-config/models/'65 AC30_6 - The Iconic Cleanish.nam"),
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

impl RigLibrary {
    /// Load the library, bootstrapping any missing file (and the NAM
    /// models the defaults reference) from the embedded in-repo default
    /// config, so the directory is always complete.
    pub fn load_or_bootstrap() -> Self {
        seed_models();
        let store = store();
        let mut profile =
            store.read_or_seed::<ProfileDef>("profile.styx", DEFAULT_PROFILE, worship_def);
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
        for preset in &mut profile.presets {
            store.resolve(&mut preset.nam);
        }
        for dp in &mut drive_presets {
            for option in &mut dp.options {
                store.resolve(&mut option.nam);
            }
        }
        Self {
            profile,
            drive_presets,
            songs,
            setlists,
            midi_map,
            keymap,
        }
    }

    pub fn save_profile(profile: &ProfileDef) {
        let store = store();
        let mut profile = profile.clone();
        for preset in &mut profile.presets {
            store.relativize(&mut preset.nam);
        }
        store.write("profile.styx", &profile);
    }

    pub fn save_drive_presets(presets: &[DrivePresetDef]) {
        let store = store();
        let mut presets = presets.to_vec();
        for dp in &mut presets {
            for option in &mut dp.options {
                store.relativize(&mut option.nam);
            }
        }
        store.write("drive-presets.styx", &DrivePresetLib { presets });
    }

    pub fn save_songs(songs: &[SongDef]) {
        store().write(
            "songs.styx",
            &SongLib {
                songs: songs.to_vec(),
            },
        );
    }

    pub fn save_setlists(setlists: &[SetlistDef]) {
        store().write(
            "setlists.styx",
            &SetlistLib {
                setlists: setlists.to_vec(),
            },
        );
    }

    pub fn save_last_state(state: &LastState) {
        store().write("last-state.styx", state);
    }

    /// `None` when the file is missing (fresh install) or unparsable.
    pub fn load_last_state() -> Option<LastState> {
        store().read("last-state.styx")
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn nam_paths_roundtrip_relative() {
        std::env::set_var("SIGNAL_RIG_DIR", "/tmp/fts-test-rig");
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

    #[test]
    fn last_state_roundtrips() {
        // Same dir as the sibling test — tests share the process env.
        std::env::set_var("SIGNAL_RIG_DIR", "/tmp/fts-test-rig");
        let state = super::LastState {
            setlist_index: 2,
            song_index: 5,
            part_index: 1,
            active_patch: "Lead Big".to_string(),
            tempo_bpm: 74.0,
        };
        super::RigLibrary::save_last_state(&state);
        let back = super::RigLibrary::load_last_state().expect("last-state.styx roundtrip");
        assert_eq!(back.setlist_index, 2);
        assert_eq!(back.song_index, 5);
        assert_eq!(back.part_index, 1);
        assert_eq!(back.active_patch, "Lead Big");
        assert_eq!(back.tempo_bpm, 74.0);
    }
}

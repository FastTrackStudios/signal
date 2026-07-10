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

use crate::profiles::{
    DrivePresetDef, KeyBindingDef, MidiMapDef, ProfileDef, SetlistDef, SongDef,
    default_keymap, default_midi_map, default_setlists, drive_presets, song_library,
    worship_def,
};

/// The library directory (`SIGNAL_RIG_DIR` overrides).
pub fn rig_dir() -> PathBuf {
    if let Ok(p) = std::env::var("SIGNAL_RIG_DIR") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    signal_sampler::rig_prefs::signal_config_dir().join("rig")
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

/// Resolve a rig-dir-relative NAM path ("models/…") to absolute;
/// absolute paths pass through.
fn resolve_nam(path: &mut String) {
    if !path.is_empty() && !std::path::Path::new(path.as_str()).is_absolute() {
        *path = rig_dir().join(path.as_str()).to_string_lossy().into_owned();
    }
}

/// Inverse of [`resolve_nam`] for saves: paths under the rig dir are
/// stored relative, so the on-disk library stays portable — including
/// when the rig dir is a symlink into the repo's default-config (live
/// edits become committable working-tree diffs).
fn relativize_nam(path: &mut String) {
    if let Ok(rel) = std::path::Path::new(path.as_str()).strip_prefix(rig_dir()) {
        *path = rel.to_string_lossy().into_owned();
    }
}

fn read<T: for<'a> Facet<'a>>(file: &str) -> Option<T> {
    let path = rig_dir().join(file);
    let text = std::fs::read_to_string(&path).ok()?;
    match facet_styx::from_str::<T>(&text) {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::warn!("rig library: {file} failed to parse ({e}) — using defaults");
            None
        }
    }
}

fn write<T: for<'a> Facet<'a>>(file: &str, value: &T) {
    let dir = rig_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!("rig library: cannot create {}: {e}", dir.display());
        return;
    }
    match facet_styx::to_string(value) {
        Ok(text) => {
            if let Err(e) = std::fs::write(dir.join(file), text) {
                tracing::warn!("rig library: write {file} failed: {e}");
            }
        }
        Err(e) => tracing::warn!("rig library: serialize {file} failed: {e}"),
    }
}

/// Read `file`, seeding it from the embedded in-repo default text when
/// missing (the text is written verbatim so the on-disk copy matches the
/// repo snapshot). Falls back to the code-built default if the embedded
/// text fails to parse.
fn read_or_seed<T: for<'a> Facet<'a>>(file: &str, seed: &str, fallback: impl FnOnce() -> T) -> T {
    if let Some(v) = read(file) {
        return v;
    }
    let dir = rig_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!("rig library: cannot create {}: {e}", dir.display());
    } else if let Err(e) = std::fs::write(dir.join(file), seed) {
        tracing::warn!("rig library: seed {file} failed: {e}");
    } else {
        tracing::info!("rig library: seeded {file} from the in-repo default");
    }
    match facet_styx::from_str::<T>(seed) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("rig library: embedded default {file} failed to parse ({e})");
            let v = fallback();
            write(file, &v);
            v
        }
    }
}

impl RigLibrary {
    /// Load the library, bootstrapping any missing file (and the NAM
    /// models the defaults reference) from the embedded in-repo default
    /// config, so the directory is always complete.
    pub fn load_or_bootstrap() -> Self {
        seed_models();
        let mut profile = read_or_seed::<ProfileDef>("profile.styx", DEFAULT_PROFILE, worship_def);
        let mut drive_presets =
            read_or_seed::<DrivePresetLib>("drive-presets.styx", DEFAULT_DRIVE_PRESETS, || {
                DrivePresetLib { presets: drive_presets() }
            })
            .presets;
        let songs = read_or_seed::<SongLib>("songs.styx", DEFAULT_SONGS, || SongLib {
            songs: song_library(),
        })
        .songs;
        let setlists =
            read_or_seed::<SetlistLib>("setlists.styx", DEFAULT_SETLISTS, || SetlistLib {
                setlists: default_setlists(),
            })
            .setlists;
        let midi_map = read_or_seed::<MidiMapDef>("midi.styx", DEFAULT_MIDI, default_midi_map);
        let keymap = read_or_seed::<KeymapLib>("keymap.styx", DEFAULT_KEYMAP, || KeymapLib {
            bindings: default_keymap(),
        })
        .bindings;
        for preset in &mut profile.presets {
            resolve_nam(&mut preset.nam);
        }
        for dp in &mut drive_presets {
            for option in &mut dp.options {
                resolve_nam(&mut option.nam);
            }
        }
        Self { profile, drive_presets, songs, setlists, midi_map, keymap }
    }

    pub fn save_profile(profile: &ProfileDef) {
        let mut profile = profile.clone();
        for preset in &mut profile.presets {
            relativize_nam(&mut preset.nam);
        }
        write("profile.styx", &profile);
    }

    pub fn save_drive_presets(presets: &[DrivePresetDef]) {
        let mut presets = presets.to_vec();
        for dp in &mut presets {
            for option in &mut dp.options {
                relativize_nam(&mut option.nam);
            }
        }
        write("drive-presets.styx", &DrivePresetLib { presets });
    }

    pub fn save_songs(songs: &[SongDef]) {
        write("songs.styx", &SongLib { songs: songs.to_vec() });
    }

    pub fn save_setlists(setlists: &[SetlistDef]) {
        write("setlists.styx", &SetlistLib { setlists: setlists.to_vec() });
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn nam_paths_roundtrip_relative() {
        std::env::set_var("SIGNAL_RIG_DIR", "/tmp/fts-test-rig");
        let mut p = String::from("models/x.nam");
        super::resolve_nam(&mut p);
        assert_eq!(p, "/tmp/fts-test-rig/models/x.nam");
        super::relativize_nam(&mut p);
        assert_eq!(p, "models/x.nam");
        let mut abs = String::from("/elsewhere/y.nam");
        super::resolve_nam(&mut abs);
        super::relativize_nam(&mut abs);
        assert_eq!(abs, "/elsewhere/y.nam");
    }
}

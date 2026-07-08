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
//! First run bootstraps the files from the built-in worship defaults, so
//! the directory is always a complete, editable snapshot.

use std::path::PathBuf;

use facet::Facet;

use crate::profiles::{
    DrivePresetDef, ProfileDef, SetlistDef, SongDef, default_setlists, drive_presets,
    song_library, worship_def,
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

/// Everything loaded from the rig directory.
#[derive(Clone, Debug)]
pub struct RigLibrary {
    pub profile: ProfileDef,
    pub drive_presets: Vec<DrivePresetDef>,
    pub songs: Vec<SongDef>,
    pub setlists: Vec<SetlistDef>,
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

impl RigLibrary {
    /// Load the library, bootstrapping any missing file from the built-in
    /// defaults (and writing it out so the directory is complete).
    pub fn load_or_bootstrap() -> Self {
        let dir = rig_dir();
        let profile = read::<ProfileDef>("profile.styx").unwrap_or_else(|| {
            let def = worship_def();
            write("profile.styx", &def);
            tracing::info!("rig library: bootstrapped profile.styx at {}", dir.display());
            def
        });
        let drive_presets = read::<DrivePresetLib>("drive-presets.styx")
            .map(|l| l.presets)
            .unwrap_or_else(|| {
                let presets = drive_presets();
                write("drive-presets.styx", &DrivePresetLib { presets: presets.clone() });
                presets
            });
        let songs = read::<SongLib>("songs.styx").map(|l| l.songs).unwrap_or_else(|| {
            let songs = song_library();
            write("songs.styx", &SongLib { songs: songs.clone() });
            songs
        });
        let setlists = read::<SetlistLib>("setlists.styx")
            .map(|l| l.setlists)
            .unwrap_or_else(|| {
                let setlists = default_setlists();
                write("setlists.styx", &SetlistLib { setlists: setlists.clone() });
                setlists
            });
        Self { profile, drive_presets, songs, setlists }
    }

    pub fn save_profile(profile: &ProfileDef) {
        write("profile.styx", profile);
    }

    pub fn save_drive_presets(presets: &[DrivePresetDef]) {
        write("drive-presets.styx", &DrivePresetLib { presets: presets.to_vec() });
    }

    pub fn save_songs(songs: &[SongDef]) {
        write("songs.styx", &SongLib { songs: songs.to_vec() });
    }

    pub fn save_setlists(setlists: &[SetlistDef]) {
        write("setlists.styx", &SetlistLib { setlists: setlists.to_vec() });
    }
}

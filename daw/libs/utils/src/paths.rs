//! Standard FTS directory paths.
//!
//! All paths are derived from a single root — `fts_home()` — which auto-discovers
//! the installation directory:
//!
//! 1. `$FTS_HOME` env var (explicit override, always wins)
//! 2. `~/Music/FastTrackStudio/` (production install, if it exists)
//! 3. `~/Music/Dev/FastTrackStudio/` (dev fallback)

use std::path::PathBuf;

/// The FTS installation root.
///
/// Resolution order:
/// 1. `$FTS_HOME` environment variable (explicit override)
/// 2. `~/Music/FastTrackStudio/` if it exists (production install)
/// 3. `~/Music/Dev/FastTrackStudio/` (dev fallback)
pub fn fts_home() -> PathBuf {
    if let Ok(p) = std::env::var("FTS_HOME") {
        return PathBuf::from(p);
    }
    if let Some(home) = home_dir() {
        let production = home.join("Music/FastTrackStudio");
        // Check for the Reaper dir with an actual .app bundle — an empty
        // skeleton created by a stray REAPER launch doesn't count.
        if production.join("Reaper/reaper.ini").exists() {
            return production;
        }
        return home.join("Music/Dev/FastTrackStudio");
    }
    PathBuf::from("/tmp/FastTrackStudio")
}

/// `<fts_home>/Library/` — signal database, presets, catalog, config files.
pub fn library_dir() -> PathBuf {
    fts_home().join("Library")
}

/// `<fts_home>/Library/signal.db`
pub fn signal_db() -> PathBuf {
    library_dir().join("signal.db")
}

/// `<fts_home>/Library/assets/`
pub fn assets_dir() -> PathBuf {
    library_dir().join("assets")
}

/// `<fts_home>/Reaper/` — portable REAPER resource directory.
pub fn reaper_dir() -> PathBuf {
    fts_home().join("Reaper")
}

/// `<fts_home>/Reaper/UserPlugins/`
pub fn reaper_user_plugins() -> PathBuf {
    reaper_dir().join("UserPlugins")
}

/// `<fts_home>/Reaper/FXChains/`
pub fn reaper_fxchains() -> PathBuf {
    reaper_dir().join("FXChains")
}

/// `<fts_home>/Reaper/TrackTemplates/`
pub fn reaper_track_templates() -> PathBuf {
    reaper_dir().join("TrackTemplates")
}

/// `<fts_home>/Plug-Ins/`
pub fn plugins_dir() -> PathBuf {
    fts_home().join("Plug-Ins")
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

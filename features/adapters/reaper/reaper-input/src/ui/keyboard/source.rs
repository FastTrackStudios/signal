//! Editor data-source mode.
//!
//! The keybind editor UI runs in two contexts:
//!
//! - **In-REAPER** (dock panel): the live processor + REAPER's action list are
//!   available, so the keyboard view can show what's currently active and the
//!   action picker can enumerate every REAPER action.
//! - **Standalone** (the `keybind-editor` Blitz binary): REAPER is *not* loaded,
//!   so any call to [`reaper_high::Reaper::get`] would panic. In this mode the
//!   editor is driven purely from the `.styx` files on disk.
//!
//! Both contexts edit the same `.styx` files via
//! [`SectionEditor`](crate::keybind_config::editor::SectionEditor) — the runtime
//! is only ever an *enrichment*. Call [`init_standalone`] once from the
//! standalone binary before launching the UI; everything else keys off
//! [`is_standalone`].

use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

/// Set in the standalone binary. Holds the FTS config dir (the directory that
/// contains the `keybinds/` and `workflows/` subdirectories).
static STANDALONE_DIR: OnceLock<PathBuf> = OnceLock::new();

/// The profile the editor is currently focused on. When `Some`, file-based
/// collection is scoped to this profile; when `None`, all profiles are merged.
static ACTIVE_PROFILE: RwLock<Option<String>> = RwLock::new(None);

/// Enter standalone (no-REAPER) mode, rooted at `config_dir`.
///
/// `config_dir` must contain a `keybinds/` subdirectory with one directory per
/// profile (e.g. `keybinds/fasttrackstudio/profile.styx`).
pub fn init_standalone(config_dir: PathBuf) {
    let _ = STANDALONE_DIR.set(config_dir);
}

/// Whether the editor is running without a live REAPER instance.
pub fn is_standalone() -> bool {
    STANDALONE_DIR.get().is_some()
}

/// Resolve the FTS config dir (the parent of `keybinds/`).
///
/// Resolution order:
/// 1. The standalone dir set via [`init_standalone`].
/// 2. The `FTS_CONFIG_DIR` environment variable.
/// 3. `<REAPER resource path>/fasttrackstudio/input` (in-REAPER only).
pub fn config_dir() -> PathBuf {
    if let Some(dir) = STANDALONE_DIR.get() {
        return dir.clone();
    }
    if let Ok(dir) = std::env::var("FTS_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    reaper_high::Reaper::get()
        .resource_path()
        .join("fasttrackstudio/input")
        .into()
}

/// The `keybinds/` directory holding per-profile config subdirectories.
pub fn keybinds_dir() -> PathBuf {
    config_dir().join("keybinds")
}

/// The profile the editor is focused on, if any.
pub fn active_profile() -> Option<String> {
    ACTIVE_PROFILE.read().ok().and_then(|g| g.clone())
}

/// Focus the editor on a single profile (or `None` to merge all profiles).
pub fn set_active_profile(profile: Option<String>) {
    if let Ok(mut g) = ACTIVE_PROFILE.write() {
        *g = profile;
    }
}

/// Directory of the active profile, if one is focused and it exists on disk.
pub fn active_profile_dir() -> Option<PathBuf> {
    let profile = active_profile()?;
    let dir = keybinds_dir().join(&profile);
    dir.join("profile.styx").exists().then_some(dir)
}

/// List the available profile IDs (subdirectory names of `keybinds/` that
/// contain a `profile.styx`), sorted alphabetically.
pub fn list_profiles() -> Vec<String> {
    let mut profiles = Vec::new();
    if let Ok(entries) = std::fs::read_dir(keybinds_dir()) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir()
                && path.join("profile.styx").exists()
                && let Some(name) = path.file_name().and_then(|n| n.to_str())
            {
                profiles.push(name.to_string());
            }
        }
    }
    profiles.sort();
    profiles
}

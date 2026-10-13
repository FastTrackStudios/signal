//! Standalone keybind editor — a native Blitz (dioxus-native) window for
//! editing FastTrackStudio `.styx` keybind profiles without REAPER running.
//!
//! # Usage
//!
//! ```text
//! keybind-editor [CONFIG_DIR]
//! ```
//!
//! `CONFIG_DIR` is the FTS config directory containing a `keybinds/`
//! subdirectory (one directory per profile). Resolution order:
//!
//! 1. The first CLI argument.
//! 2. The `FTS_CONFIG_DIR` environment variable.
//! 3. `$REAPER_HOME/fasttrackstudio/input`.
//! 4. `~/fts-dev/fasttrackstudio/input`.

use std::path::PathBuf;

use reaper_input::ui::keyboard::EditorApp;
use reaper_input::ui::keyboard::source;

fn resolve_config_dir() -> PathBuf {
    if let Some(arg) = std::env::args().nth(1) {
        return PathBuf::from(arg);
    }
    if let Ok(env) = std::env::var("FTS_CONFIG_DIR") {
        return PathBuf::from(env);
    }
    if let Ok(home) = std::env::var("REAPER_HOME") {
        return PathBuf::from(home).join("fasttrackstudio/input");
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join("fts-dev/fasttrackstudio/input")
}

fn main() {
    let config_dir = resolve_config_dir();
    let keybinds = config_dir.join("keybinds");
    if !keybinds.is_dir() {
        eprintln!(
            "warning: no 'keybinds/' directory under {} — the editor will be empty.\n\
             Pass a config dir as the first argument or set FTS_CONFIG_DIR.",
            config_dir.display()
        );
    } else {
        tracing::info!(config_dir = %config_dir.display(), "Editing keybind profiles");
    }

    source::init_standalone(config_dir);

    dioxus_native::launch(EditorApp);
}

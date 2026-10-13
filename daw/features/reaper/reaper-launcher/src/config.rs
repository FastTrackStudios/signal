//! Typed configuration for REAPER instances.
//!
//! Maps REAPER ini variable names to Rust types. Each wrapper `.app`
//! stores a `launch.json` in its `Contents/` directory that the
//! launcher binary reads at startup.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Known `reaper.ini` config keys (from REAPER API docs).
#[allow(dead_code)]
pub mod keys {
    /// Maximum undo memory in MB. 0 disables undo and save prompts.
    pub const UNDO_MAX_MEM: &str = "undomaxmem";
    /// Bitfield controlling which actions are undoable.
    pub const UNDO_MASK: &str = "undomask";
    /// Whether to save undo states in project files.
    pub const SAVE_UNDO_STATES_PROJ: &str = "saveundostatesproj";
    /// Currently loaded theme (full path or "<classic>").
    pub const LAST_THEME: &str = "lastthemefn5";
    /// Auto-save interval in seconds.
    pub const AUTO_SAVE_INTERVAL: &str = "autosaveint";
    /// Auto-save mode (when recording/stopped/any).
    pub const AUTO_SAVE_MODE: &str = "autosavemode";
}

/// reaper.ini overrides to apply before launching.
/// Only `Some` values are patched; `None` means "leave as-is".
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReaperIniConfig {
    /// Maximum undo memory in MB. 0 disables undo and close-save prompts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub undo_max_mem: Option<u32>,
    /// Currently loaded theme (full path to .ReaperThemeZip).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
}

impl ReaperIniConfig {
    /// Convert to a list of (key, value) pairs for patching.
    pub fn as_patches(&self) -> Vec<(&str, String)> {
        let mut patches = Vec::new();
        if let Some(mem) = self.undo_max_mem {
            patches.push((keys::UNDO_MAX_MEM, mem.to_string()));
        }
        if let Some(ref theme) = self.theme {
            patches.push((keys::LAST_THEME, theme.clone()));
        }
        patches
    }
}

fn default_reaper_args() -> Vec<String> {
    vec![
        "-newinst".to_string(),
        "-nosplash".to_string(),
        "-ignoreerrors".to_string(),
    ]
}

/// Full launch configuration for a wrapper .app / Linux rig.
/// Stored as `Contents/launch.json` (macOS) or
/// `~/.config/fts/rigs/{id}/launch.json` (Linux).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LaunchConfig {
    /// DAW role: "signal", "session", or "testing".
    pub role: String,
    /// Rig type (only for signal instances).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rig_type: Option<String>,
    /// Path to the real REAPER executable.
    pub reaper_executable: String,
    /// Path to REAPER's resource directory (working dir and UserPlugins parent).
    pub resources_dir: String,
    /// Path to reaper.ini.
    pub ini_path: String,
    /// INI overrides to apply before launching.
    #[serde(default)]
    pub ini_overrides: ReaperIniConfig,
    /// Whether to restore original INI values after REAPER starts.
    /// When true, the launcher forks: child execs REAPER, parent
    /// waits briefly then restores the original values.
    #[serde(default)]
    pub restore_ini_after_launch: bool,
    /// Arguments passed to the REAPER executable on launch.
    /// Defaults to `["-newinst", "-nosplash", "-ignoreerrors"]`.
    /// Extra CLI args passed to the launcher are appended after these.
    #[serde(default = "default_reaper_args")]
    pub reaper_args: Vec<String>,
}

impl LaunchConfig {
    /// Return the standard REAPER launch arguments.
    pub fn standard_reaper_args() -> Vec<String> {
        default_reaper_args()
    }
}

impl LaunchConfig {
    /// Load from a `launch.json` file.
    ///
    /// Supports two formats:
    /// - Single rig: `{ "role": "signal", ... }`
    /// - Multi-rig: `{ "fts-keys": { "role": "signal", ... }, ... }`
    ///
    /// For multi-rig files, pass `rig_id` to select which rig to load.
    pub fn load(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse {}: {e}", path.display()))
    }

    /// Load a specific rig from a multi-rig `launch.json`.
    ///
    /// The file is a JSON object keyed by rig id:
    /// ```json
    /// {
    ///   "fts-keys": { "role": "signal", "rig_type": "keys", ... },
    ///   "fts-drums": { "role": "signal", "rig_type": "drums", ... }
    /// }
    /// ```
    pub fn load_rig(path: &Path, rig_id: &str) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;

        // Try parsing as multi-rig first
        if let Ok(map) = serde_json::from_str::<std::collections::HashMap<String, Self>>(&content) {
            return map
                .into_iter()
                .find(|(k, _)| k == rig_id)
                .map(|(_, v)| v)
                .ok_or_else(|| format!("Rig '{rig_id}' not found in {}", path.display()));
        }

        // Fall back to single-rig format
        serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse {}: {e}", path.display()))
    }

    /// Save to a `launch.json` file.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let json =
            serde_json::to_string_pretty(self).map_err(|e| format!("Failed to serialize: {e}"))?;
        std::fs::write(path, json).map_err(|e| format!("Failed to write {}: {e}", path.display()))
    }
}

//! Load keybind profiles/sections from styx into an `input::KeymapConfig`.
//!
//! The app-agnostic slice of reaper-input's `keybind_config/loader.rs`:
//! parse `profile.styx` (a [`ProfileConfig`]) + its section files (each a
//! [`SectionConfig`]) and bridge the accumulated bindings / which-key trees /
//! wheel bindings into a [`KeymapConfig`]. No REAPER-specific overlay,
//! workflow, or mouse-modifier handling — those stay in reaper-input.

use std::collections::HashMap;
use std::path::Path;

use input::config::KeymapConfig;
use input_config_proto::{KeybindDef, ProfileConfig, SectionConfig, WheelBindDef, WhichKeyTreeDef};

use crate::bridge::{keymap_config_from_defs, section_to_keymap_config};

/// Parse a single section's styx source into a [`KeymapConfig`].
///
/// Handy for the web build, which fetches styx section strings rather than
/// reading a directory off disk.
pub fn keymap_from_section_str(styx: &str) -> Result<KeymapConfig, String> {
    let section: SectionConfig = facet_styx::from_str(styx).map_err(|e| e.to_string())?;
    Ok(section_to_keymap_config(&section))
}

/// A profile, loaded: the keymap to run, and the which-key labels the
/// keymap itself does not keep (see [`crate::bridge::which_key_labels`]).
#[derive(Debug, Clone, Default)]
pub struct Profile {
    pub keymap: KeymapConfig,
    pub labels: HashMap<Vec<input::KeyChord>, String>,
}

/// Load a [`KeymapConfig`] from a profile directory.
///
/// `dir` should contain a `profile.styx` naming the section files to load, in
/// order (later files win on key conflicts). Returns `None` if `profile.styx`
/// cannot be read or parsed. Unreadable/unparseable section files are skipped.
pub fn load_profile_keymap(dir: &Path) -> Option<KeymapConfig> {
    load_profile_dir(dir).map(|profile| profile.keymap)
}

/// [`load_profile_keymap`], with the which-key labels.
pub fn load_profile_dir(dir: &Path) -> Option<Profile> {
    load_profile_from(|name| std::fs::read_to_string(dir.join(name)).ok())
}

/// Load a profile from any source of its files, by file name
/// (`profile.styx`, then each section it lists): a directory, or the
/// copy compiled in ([`crate::embedded`]).
pub fn load_profile_from(read: impl Fn(&str) -> Option<String>) -> Option<Profile> {
    let profile = facet_styx::from_str::<ProfileConfig>(&read("profile.styx")?).ok()?;

    let mut bindings: Vec<KeybindDef> = Vec::new();
    let mut trees: Vec<WhichKeyTreeDef> = Vec::new();
    let mut wheel: Vec<WheelBindDef> = Vec::new();

    for filename in &profile.sections {
        let section = read(filename).and_then(|s| facet_styx::from_str::<SectionConfig>(&s).ok());
        if let Some(section) = section {
            bindings.extend(section.bindings().iter().cloned());
            trees.extend(section.which_key().iter().cloned());
            wheel.extend(section.wheel().iter().cloned());
        }
    }

    Some(Profile {
        keymap: keymap_config_from_defs(&bindings, &trees, &wheel),
        labels: crate::bridge::which_key_labels(&trees),
    })
}

fn load_profile(dir: &Path) -> Option<ProfileConfig> {
    let contents = std::fs::read_to_string(dir.join("profile.styx")).ok()?;
    facet_styx::from_str::<ProfileConfig>(&contents).ok()
}

/// List every profile under `root` — one subdirectory per profile, each
/// containing its own `profile.styx` (the same layout `load_profile_keymap`
/// reads one of). The slug (subdirectory name — `"fasttrackstudio"`,
/// `"logic"`, …) is what a caller would pass back into
/// `load_profile_keymap(root.join(slug))` to actually load it; `name`/
/// `description` on the returned [`ProfileConfig`] are what a picker UI
/// shows. Unreadable directories and non-profile subdirectories (no
/// `profile.styx`) are skipped rather than erroring — `root` also holds
/// shared, non-profile folders (`workflows/`, `overlays/`).
pub fn list_profiles(root: &Path) -> Vec<(String, ProfileConfig)> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut profiles: Vec<(String, ProfileConfig)> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|entry| {
            let slug = entry.file_name().to_string_lossy().into_owned();
            let profile = load_profile(&entry.path())?;
            Some((slug, profile))
        })
        .collect();
    profiles.sort_by(|a, b| a.1.name.to_lowercase().cmp(&b.1.name.to_lowercase()));
    profiles
}

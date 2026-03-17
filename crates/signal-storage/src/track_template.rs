//! Track template writer for REAPER-native signal presets.
//!
//! Writes `.RTrackTemplate` files + `.signal.styx` sidecars to the
//! `TrackTemplates/FTS-Signal/{instrument}/` directory tree.
//!
//! # Directory layout
//!
//! ```text
//! TrackTemplates/FTS-Signal/
//! ├── Guitar/
//! │   ├── 01-Layers/
//! │   │   └── Clean Layer.RTrackTemplate
//! │   ├── 02-Engines/
//! │   ├── 03-Rigs/
//! │   │   └── Guitar Rig.RTrackTemplate
//! │   ├── 04-Profiles/
//! │   └── 05-Songs/
//! └── Racks/
//!     ├── 04-Profiles/
//!     └── 05-Songs/
//! ```

use std::path::{Path, PathBuf};

use crate::sidecar::{self, PresetKind, SignalSidecar};

// ─── Constants ────────────────────────────────────────────────

const FTS_SIGNAL_DIR: &str = "FTS-Signal";
const LAYERS_DIR: &str = "01-Layers";
const ENGINES_DIR: &str = "02-Engines";
const RIGS_DIR: &str = "03-Rigs";
const PROFILES_DIR: &str = "04-Profiles";
const SONGS_DIR: &str = "05-Songs";

// ─── Instrument ───────────────────────────────────────────────

/// Instrument folder for the TrackTemplates directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Instrument {
    Guitar,
    Bass,
    Vocal,
    Keys,
    Drums,
    DrumEnhance,
}

impl Instrument {
    pub fn folder_name(self) -> &'static str {
        match self {
            Self::Guitar => "Guitar",
            Self::Bass => "Bass",
            Self::Vocal => "Vocal",
            Self::Keys => "Keys",
            Self::Drums => "Drums",
            Self::DrumEnhance => "Drum-Enhance",
        }
    }
}

// ─── Preset tier ──────────────────────────────────────────────

/// Which tier of the signal hierarchy this template represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateTier {
    Layer,
    Engine,
    Rig,
    Profile,
    Song,
}

impl TemplateTier {
    fn dir_name(self) -> &'static str {
        match self {
            Self::Layer => LAYERS_DIR,
            Self::Engine => ENGINES_DIR,
            Self::Rig => RIGS_DIR,
            Self::Profile => PROFILES_DIR,
            Self::Song => SONGS_DIR,
        }
    }

    fn to_preset_kind(self) -> PresetKind {
        match self {
            Self::Layer => PresetKind::Layer,
            Self::Engine => PresetKind::Engine,
            Self::Rig => PresetKind::Rig,
            Self::Profile => PresetKind::Profile,
            Self::Song => PresetKind::Song,
        }
    }
}

// ─── TrackTemplateWriter ──────────────────────────────────────

/// Resolve the FTS-Signal TrackTemplates root directory.
pub fn track_templates_root() -> PathBuf {
    utils::paths::reaper_track_templates().join(FTS_SIGNAL_DIR)
}

/// Save a track template variation to the FTS-Signal directory.
///
/// Each preset is a **folder** containing its variations as individual
/// `.RTrackTemplate` files. The first save should use `variation_name = "Default"`.
///
/// # Directory layout
///
/// ```text
/// Guitar/01-Layers/
/// ├── Guitar Full Chain/           ← preset folder (name)
/// │   ├── Default.RTrackTemplate   ← default variation
/// │   ├── Default.signal.styx
/// │   ├── Bright.RTrackTemplate    ← additional variation
/// │   └── Bright.signal.styx
/// ```
///
/// # Arguments
/// * `preset_name` — Name of the preset (becomes the folder)
/// * `variation_name` — Name of this variation (becomes the filename)
/// * `instrument` — Which instrument folder to save under
/// * `tier` — Layer, Engine, Rig, Profile, or Song
/// * `track_chunks` — The raw RPP `<TRACK ...>` block(s) to save
/// * `id` — Stable UUID for this variation
/// * `tags` — Freeform tags for the sidecar
/// * `description` — Optional description
///
/// # Returns
/// The path to the written `.RTrackTemplate` file.
pub fn save_track_template(
    preset_name: &str,
    variation_name: &str,
    instrument: Instrument,
    tier: TemplateTier,
    track_chunks: &str,
    id: &str,
    tags: &[String],
    description: Option<&str>,
) -> std::io::Result<PathBuf> {
    let root = track_templates_root();
    let dir = root
        .join(instrument.folder_name())
        .join(tier.dir_name())
        .join(sanitize_filename(preset_name));
    std::fs::create_dir_all(&dir)?;

    let file_name = sanitize_filename(variation_name);
    let template_path = dir.join(format!("{file_name}.RTrackTemplate"));

    // Write the track template
    std::fs::write(&template_path, track_chunks)?;

    // Write the sidecar
    let sc = SignalSidecar {
        version: 1,
        id: id.to_string(),
        kind: tier.to_preset_kind(),
        tags: tags.to_vec(),
        description: description.map(String::from),
        parameters: vec![],
    };
    sidecar::write_sidecar(&template_path, &sc)?;

    Ok(template_path)
}

/// Save a rack template variation (multi-instrument, under `Racks/`).
pub fn save_rack_template(
    preset_name: &str,
    variation_name: &str,
    tier: TemplateTier,
    track_chunks: &str,
    id: &str,
    tags: &[String],
    description: Option<&str>,
) -> std::io::Result<PathBuf> {
    let root = track_templates_root();
    let dir = root
        .join("Racks")
        .join(tier.dir_name())
        .join(sanitize_filename(preset_name));
    std::fs::create_dir_all(&dir)?;

    let file_name = sanitize_filename(variation_name);
    let template_path = dir.join(format!("{file_name}.RTrackTemplate"));

    std::fs::write(&template_path, track_chunks)?;

    let sc = SignalSidecar {
        version: 1,
        id: id.to_string(),
        kind: PresetKind::Rack,
        tags: tags.to_vec(),
        description: description.map(String::from),
        parameters: vec![],
    };
    sidecar::write_sidecar(&template_path, &sc)?;

    Ok(template_path)
}

fn sanitize_filename(name: &str) -> String {
    name.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
}

// ─── Scanner ──────────────────────────────────────────────────

/// A scanned track template entry.
#[derive(Debug)]
pub struct ScannedTemplate {
    /// Path to the `.RTrackTemplate` file.
    pub path: PathBuf,
    /// Preset name (folder name).
    pub preset_name: String,
    /// Variation name (file stem).
    pub variation_name: String,
    /// Instrument folder (e.g., "Guitar", "Bass").
    pub instrument: Option<String>,
    /// Tier within the hierarchy.
    pub tier: Option<TemplateTier>,
    /// Parsed sidecar metadata, if present.
    pub sidecar: Option<SignalSidecar>,
}

/// Scan the TrackTemplates/FTS-Signal/ directory for all `.RTrackTemplate` files.
///
/// Handles the nested structure:
/// `{instrument}/{tier}/{preset_name}/{variation_name}.RTrackTemplate`
pub fn scan_track_templates(root: &Path) -> Vec<ScannedTemplate> {
    let mut results = Vec::new();

    if !root.is_dir() {
        return results;
    }

    let Ok(entries) = std::fs::read_dir(root) else {
        return results;
    };

    for entry in entries.flatten() {
        let instrument_dir = entry.path();
        if !instrument_dir.is_dir() {
            continue;
        }
        let instrument_name = instrument_dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let Ok(tier_entries) = std::fs::read_dir(&instrument_dir) else {
            continue;
        };

        for tier_entry in tier_entries.flatten() {
            let tier_dir = tier_entry.path();
            if !tier_dir.is_dir() {
                continue;
            }

            let tier_name = tier_dir
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let tier = match tier_name.as_str() {
                LAYERS_DIR => Some(TemplateTier::Layer),
                ENGINES_DIR => Some(TemplateTier::Engine),
                RIGS_DIR => Some(TemplateTier::Rig),
                PROFILES_DIR => Some(TemplateTier::Profile),
                SONGS_DIR => Some(TemplateTier::Song),
                _ => None,
            };

            // Scan preset folders within each tier
            let Ok(preset_entries) = std::fs::read_dir(&tier_dir) else {
                continue;
            };

            for preset_entry in preset_entries.flatten() {
                let preset_dir = preset_entry.path();
                if !preset_dir.is_dir() {
                    continue;
                }

                let preset_name = preset_dir
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();

                // Scan variation files within each preset folder
                let Ok(files) = std::fs::read_dir(&preset_dir) else {
                    continue;
                };

                for file_entry in files.flatten() {
                    let path = file_entry.path();
                    if path
                        .extension()
                        .map_or(true, |e| !e.eq_ignore_ascii_case("rtracktemplate"))
                    {
                        continue;
                    }

                    let variation_name = path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();

                    let sc = sidecar::read_sidecar(&path);

                    results.push(ScannedTemplate {
                        path,
                        preset_name: preset_name.clone(),
                        variation_name,
                        instrument: Some(instrument_name.clone()),
                        tier,
                        sidecar: sc,
                    });
                }
            }
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instrument_folder_names() {
        assert_eq!(Instrument::Guitar.folder_name(), "Guitar");
        assert_eq!(Instrument::DrumEnhance.folder_name(), "Drum-Enhance");
    }

    #[test]
    fn tier_dir_names() {
        assert_eq!(TemplateTier::Layer.dir_name(), "01-Layers");
        assert_eq!(TemplateTier::Rig.dir_name(), "03-Rigs");
        assert_eq!(TemplateTier::Song.dir_name(), "05-Songs");
    }

    #[test]
    fn sanitize_removes_special_chars() {
        assert_eq!(sanitize_filename("My/Preset:Cool"), "My_Preset_Cool");
        assert_eq!(sanitize_filename("Normal Name"), "Normal Name");
    }
}

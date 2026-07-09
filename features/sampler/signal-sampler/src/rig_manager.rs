//! Rig Manager — the saved settings for one specific rig.
//!
//! A [`RigManager`] is everything Signal remembers about a rig between sessions:
//! its **audio interface + input channel** (e.g. the Yamaha TF1's 4th input as
//! the guitar DI), the **profile** it loads (the patches), and performance
//! settings (level-match). It persists to a styx file under
//! `$XDG_CONFIG_HOME/signal/rigs/<slug>.styx` and can [`open`](RigManager::open)
//! a live [`ProfileRig`] in one call.
//!
//! ```no_run
//! use signal_sampler::RigManager;
//! let mgr = RigManager::load("Guitar Rig");      // remembered settings
//! let mut rig = mgr.open()?;                      // opens audio + loads patches
//! rig.activate_named("Lead");                     // switch while playing
//! # Ok::<(), eyre::Report>(())
//! ```

use std::path::{Path, PathBuf};

use facet::Facet;

use crate::SamplerError;
use crate::rig::GuitarRig;
use crate::rig_library::Library;
use crate::rig_prefs::{RigAudioPrefs, signal_config_dir};
use crate::rig_profile::{ProfileRig, RigProfile};

/// Saved settings for a specific rig.
#[derive(Clone, Debug, Facet)]
pub struct RigManager {
    /// Display name, e.g. "Guitar Rig". Also the file slug.
    pub name: String,
    /// Audio I/O: device, input channel, sample rate, buffer.
    #[facet(default)]
    pub audio: RigAudioPrefs,
    /// Legacy single-file profile (`.styx`) this rig loads — its patches. Empty
    /// = none. Relative paths resolve against the manager file's directory.
    /// Superseded by the library (`library_path` + `library_profile`); kept as a
    /// fallback for hand-written inline-chain profiles.
    #[facet(default)]
    pub profile_path: String,
    /// Directory of the Preset/Profile/Song **library** (`presets/ profiles/
    /// songs/`). Empty = use the default ([`library_root`](Self::library_root)).
    #[facet(default)]
    pub library_path: String,
    /// Name of the library **profile** to open. Empty = the library's first
    /// profile (so a default Guitar Rig opens "Worship").
    #[facet(default)]
    pub library_profile: String,
    /// Auto level-match patches from measured NAM loudness (LUFS) so every amp,
    /// clean or high-gain, is the same average volume. On by default.
    #[facet(default = true)]
    pub level_match: bool,
    /// Target loudness (dB) when level-matching.
    #[facet(default = -18.0f32)]
    pub target_loudness_db: f32,
    /// Feed each NAM model the analog input level (dBu) it was captured at
    /// (authentic drive). Off by default — needs a correct interface value.
    #[facet(default)]
    pub calibrated_input: bool,
    /// Interface input calibration: the analog level (dBu) that equals 0 dBFS at
    /// the DI input. Used with the model's `input_level` for input staging.
    #[facet(default = 12.0f32)]
    pub input_calibration_dbu: f32,
}

impl Default for RigManager {
    fn default() -> Self {
        Self {
            name: "Guitar Rig".to_string(),
            audio: RigAudioPrefs::default(),
            profile_path: String::new(),
            library_path: String::new(),
            library_profile: String::new(),
            level_match: true,
            target_loudness_db: -18.0,
            calibrated_input: false,
            input_calibration_dbu: 12.0,
        }
    }
}

/// Lowercase, dash-separated file slug for a rig name.
fn slugify(name: &str) -> String {
    let mut s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    while s.contains("--") {
        s = s.replace("--", "-");
    }
    s.trim_matches('-').to_string()
}

impl RigManager {
    /// Directory rig settings are stored in: `<config>/signal/rigs`.
    pub fn rigs_dir() -> PathBuf {
        signal_config_dir().join("rigs")
    }

    /// File path for a rig of the given name.
    pub fn path_for(name: &str) -> PathBuf {
        Self::rigs_dir().join(format!("{}.styx", slugify(name)))
    }

    /// This manager's file path.
    pub fn config_path(&self) -> PathBuf {
        Self::path_for(&self.name)
    }

    /// Load a rig's settings by name, or a fresh default (named) if not saved.
    pub fn load(name: &str) -> Self {
        match Self::load_from(&Self::path_for(name)) {
            Ok(m) => m,
            Err(_) => Self {
                name: name.to_string(),
                ..Self::default()
            },
        }
    }

    /// List the names of all saved rigs.
    pub fn list() -> Vec<String> {
        let mut names = Vec::new();
        if let Ok(rd) = std::fs::read_dir(Self::rigs_dir()) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().and_then(|x| x.to_str()) == Some("styx") {
                    if let Ok(m) = Self::load_from(&p) {
                        names.push(m.name);
                    }
                }
            }
        }
        names.sort();
        names
    }

    pub fn load_from(path: &Path) -> Result<Self, SamplerError> {
        let text = std::fs::read_to_string(path)?;
        facet_styx::from_str(&text).map_err(|e| SamplerError::SpecParse(e.to_string()))
    }

    /// Save these settings to [`config_path`](Self::config_path).
    pub fn save(&self) -> Result<(), String> {
        self.save_to(&self.config_path())
    }

    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let text = facet_styx::to_string(self).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| e.to_string())
    }

    /// Directory of the library this rig browses/loads from: `library_path` if
    /// set, else the user library (`<config>/signal/library`) if it exists, else
    /// the shipped example library (`signal-sampler/examples/library`).
    pub fn library_root(&self) -> PathBuf {
        if !self.library_path.is_empty() {
            return PathBuf::from(&self.library_path);
        }
        let user = signal_config_dir().join("library");
        if user.is_dir() {
            return user;
        }
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/library")
    }

    /// Build the library and resolve the chosen profile into a playable
    /// [`RigProfile`] (preset/scene references inlined). Returns it with the
    /// library root (the base dir for relative block paths). `None` when the
    /// library has no profiles. Picks `library_profile`, else the first profile.
    fn resolve_library_profile(&self) -> Option<(RigProfile, PathBuf)> {
        let root = self.library_root();
        let lib = Library::load_dir(&root);
        let profile = if self.library_profile.is_empty() {
            lib.profiles.first()
        } else {
            lib.profile(&self.library_profile)
        }?;
        Some((lib.resolve_profile(profile), root))
    }

    /// Resolve a (possibly relative) profile path against the manager file dir.
    fn resolved_profile_path(&self) -> Option<PathBuf> {
        if self.profile_path.is_empty() {
            return None;
        }
        let p = PathBuf::from(&self.profile_path);
        if p.is_absolute() {
            Some(p)
        } else {
            Some(
                self.config_path()
                    .parent()
                    .unwrap_or(Path::new("."))
                    .join(p),
            )
        }
    }

    /// Open a live rig from these settings: open the audio interface on the
    /// configured input channel, load the profile (if any), and apply
    /// level-match. Returns a ready-to-play [`ProfileRig`].
    pub fn open(&self) -> eyre::Result<ProfileRig> {
        let rig = GuitarRig::open(&self.audio)?;
        let mut prig = ProfileRig::new(rig);
        prig.set_target_loudness_db(self.target_loudness_db);
        prig.set_input_calibration_dbu(self.input_calibration_dbu);
        prig.set_calibrated_input(self.calibrated_input);

        // Prefer the library (resolves preset/scene references into chains),
        // unless the rig explicitly pins a legacy `profile_path` and makes no
        // library choice — then honor the legacy file. A default rig (no
        // `profile_path`) opens the library's first profile (e.g. "Worship").
        let want_library = !self.library_path.is_empty()
            || !self.library_profile.is_empty()
            || self.profile_path.is_empty();
        let mut loaded = false;
        if want_library {
            if let Some((profile, base)) = self.resolve_library_profile() {
                match prig.load_profile(profile, Some(&base)) {
                    Ok(()) => loaded = true,
                    Err(e) => {
                        tracing::warn!(error = %e, "RigManager: library profile load failed")
                    }
                }
            }
        }
        if !loaded {
            if let Some(path) = self.resolved_profile_path() {
                if let Err(e) = prig.load_profile_file(&path) {
                    tracing::warn!(path = %path.display(), error = %e, "RigManager: profile load failed");
                }
            }
        }

        // Apply after load so it re-activates the default patch level-matched.
        prig.set_level_match(self.level_match);
        Ok(prig)
        // NB: level-match uses each model's *measured* loudness from the DI
        // calibration pass (see `nam_calibrate`), so the guarantee holds even
        // for models without `loudness` metadata.
    }

    /// Update this manager's `audio` to the rig's actually-opened devices, so a
    /// subsequent [`save`](Self::save) remembers exactly what was used.
    pub fn remember_opened(&mut self, rig: &GuitarRig) {
        self.audio = rig.prefs().clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_styx() {
        let mgr = RigManager {
            name: "Guitar Rig".into(),
            audio: RigAudioPrefs {
                input_device: "Yamaha TF".into(),
                input_channel: 3,
                output_device: String::new(),
                sample_rate: 48_000,
                buffer_size: 256,
                ..Default::default()
            },
            profile_path: "testing.styx".into(),
            library_path: String::new(),
            library_profile: String::new(),
            level_match: true,
            target_loudness_db: -18.0,
            calibrated_input: true,
            input_calibration_dbu: 13.5,
        };
        let dir = std::env::temp_dir().join(format!("signal-rigmgr-{}", std::process::id()));
        let path = dir.join("guitar-rig.styx");
        mgr.save_to(&path).expect("save");
        let back = RigManager::load_from(&path).expect("load");
        assert_eq!(back.name, "Guitar Rig");
        assert_eq!(back.audio.input_device, "Yamaha TF");
        assert_eq!(back.audio.input_channel, 3);
        assert!(back.level_match);
        assert!(back.calibrated_input);
        assert_eq!(back.input_calibration_dbu, 13.5);
        assert_eq!(back.profile_path, "testing.styx");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolves_worship_from_shipped_library() {
        let mgr = RigManager {
            name: "Guitar Rig".into(),
            library_path: concat!(env!("CARGO_MANIFEST_DIR"), "/examples/library").into(),
            library_profile: "Worship".into(),
            ..Default::default()
        };
        let (profile, base) = mgr
            .resolve_library_profile()
            .expect("Worship resolves from the library");
        assert_eq!(profile.name, "Worship");
        assert_eq!(profile.stacks.len(), 8);
        assert!(base.ends_with("examples/library"));
        // Patches are resolved to inline chains (amp NAM first).
        assert!(
            profile.patches.iter().all(|p| p.preset.is_empty()),
            "all patch refs inlined"
        );
        assert!(
            profile.patches[0]
                .chain
                .first()
                .map(|b| b.is_nam())
                .unwrap_or(false)
        );
    }

    #[test]
    fn slugify_makes_filename_safe() {
        assert_eq!(slugify("Guitar Rig"), "guitar-rig");
        assert_eq!(slugify("Keys / Synth #2"), "keys-synth-2");
    }

    #[test]
    fn load_missing_returns_named_default() {
        let m = RigManager::load("Totally Nonexistent Rig 9z");
        assert_eq!(m.name, "Totally Nonexistent Rig 9z");
        assert_eq!(m.audio.input_channel, 0);
    }
}

//! Loading a preset library from disk.
//!
//! Reads the JSON that `signal-analyzer`'s `reverb_match --save-dir` writes:
//! a translated preset plus the measurements that justify it. That file is
//! the output of a full plugin-hosted tuning pass, so it is the library
//! format rather than an export of one.
//!
//! Loading is deliberately forgiving. A library is a directory someone drops
//! files into, and one unreadable or half-written file should cost that
//! preset, not the whole bank — [`LoadReport`] carries what was skipped so a
//! UI can say so instead of silently showing a short list.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::Preset;

/// Why a directory could not be read at all.
#[derive(Debug)]
pub enum LoadError {
    /// The directory itself could not be listed.
    Unreadable { path: PathBuf, source: std::io::Error },
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreadable { path, source } => {
                write!(f, "could not read {}: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for LoadError {}

/// What a load produced, including what it could not read.
#[derive(Debug, Default)]
pub struct LoadReport {
    pub presets: Vec<Preset>,
    /// `(file, why)` for each file that was skipped.
    pub skipped: Vec<(PathBuf, String)>,
}

impl LoadReport {
    #[must_use] 
    pub const fn is_empty(&self) -> bool {
        self.presets.is_empty()
    }
}

// ── The on-disk shape written by `reverb_match --save-dir` ─────────────────

#[derive(Deserialize)]
struct SavedPreset {
    source: SavedSource,
    target: SavedTarget,
    #[serde(default)]
    measurement: Option<SavedMeasurement>,
}

#[derive(Deserialize)]
struct SavedSource {
    preset: String,
    #[serde(default)]
    plugin: Option<String>,
    #[serde(default)]
    mode: Option<String>,
}

#[derive(Deserialize)]
struct SavedTarget {
    #[serde(default)]
    parameters: Vec<SavedParam>,
}

#[derive(Deserialize)]
struct SavedParam {
    name: String,
    value: f64,
}

#[derive(Deserialize)]
struct SavedMeasurement {
    #[serde(default)]
    decay_passed: Option<bool>,
    #[serde(default)]
    worst_band_ratio_error: Option<f64>,
}

impl SavedPreset {
    fn into_preset(self) -> Preset {
        // The source plugin's own mode name is the most useful grouping we
        // have — "Plate", "Chamber1979", "Large Chamber" — because it is what
        // the preset was actually voiced as, not a folder someone filed it in.
        let match_error = self.measurement.as_ref().and_then(|m| m.worst_band_ratio_error);
        let mut tags = Vec::new();
        if let Some(m) = &self.measurement {
            // Whether the translation was verified against the reference is
            // worth surfacing: a browser can show which presets are known to
            // match and which are best-effort.
            match m.decay_passed {
                Some(true) => tags.push("verified".to_string()),
                Some(false) => tags.push("approximate".to_string()),
                None => tags.push("unmeasured".to_string()),
            }
        }
        Preset {
            name: self.source.preset,
            category: self.source.mode,
            author: None,
            tags,
            origin: self.source.plugin,
            parameters: self
                .target
                .parameters
                .into_iter()
                .map(|p| (p.name, p.value))
                .collect(),
            match_error,
        }
    }
}

/// Load every `*.json` preset in a directory (non-recursive).
///
/// Returns a report rather than a bare `Vec`, so a caller can distinguish
/// "this bank is empty" from "this bank is broken".
pub fn load_directory(dir: impl AsRef<Path>) -> Result<LoadReport, LoadError> {
    let dir = dir.as_ref();
    let entries = std::fs::read_dir(dir).map_err(|source| LoadError::Unreadable {
        path: dir.to_path_buf(),
        source,
    })?;

    let mut report = LoadReport::default();
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("json")))
        .collect();
    // Stable order, so `SortMode::Library` means something reproducible.
    paths.sort();

    for path in paths {
        match std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| serde_json::from_str::<SavedPreset>(&text).map_err(|e| e.to_string()))
        {
            Ok(saved) => report.presets.push(saved.into_preset()),
            Err(why) => report.skipped.push((path, why)),
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("preset-browser-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(dir: &Path, name: &str, body: &str) {
        std::fs::write(dir.join(name), body).unwrap();
    }

    const ONE: &str = r#"{
      "source": { "preset": "Snare Plate", "plugin": "VintageVerb", "mode": "Dirty Plate" },
      "target": { "parameters": [
        { "name": "algorithm", "value": 2.0 },
        { "name": "decay_time", "value": 1.94 }
      ] },
      "measurement": { "decay_passed": true, "worst_band_ratio_error": 0.012 }
    }"#;

    #[test]
    fn loads_a_saved_preset_into_the_browser_model() {
        let dir = temp_dir("one");
        write(&dir, "snare.json", ONE);

        let report = load_directory(&dir).unwrap();
        assert!(report.skipped.is_empty());
        assert_eq!(report.presets.len(), 1);

        let p = &report.presets[0];
        assert_eq!(p.name, "Snare Plate");
        // The plugin's own mode is the grouping, not the folder it sat in.
        assert_eq!(p.category.as_deref(), Some("Dirty Plate"));
        assert_eq!(p.origin.as_deref(), Some("VintageVerb"));
        assert_eq!(p.parameters.len(), 2);
        assert_eq!(p.parameters[1], ("decay_time".to_string(), 1.94));
        assert!(p.tags.contains(&"verified".to_string()));
        // How well it matches is carried through, not just whether it passed.
        assert_eq!(p.match_error, Some(0.012));
    }

    #[test]
    fn a_broken_file_costs_only_itself() {
        let dir = temp_dir("broken");
        write(&dir, "good.json", ONE);
        write(&dir, "truncated.json", "{ \"source\": {");
        write(&dir, "notes.txt", "ignored, not json");

        let report = load_directory(&dir).unwrap();
        assert_eq!(report.presets.len(), 1, "the good preset still loads");
        assert_eq!(report.skipped.len(), 1, "and the bad one is reported");
        assert!(report.skipped[0].0.ends_with("truncated.json"));
    }

    #[test]
    fn an_unverified_preset_is_tagged_as_such() {
        let dir = temp_dir("approx");
        write(
            &dir,
            "a.json",
            &ONE.replace("\"decay_passed\": true", "\"decay_passed\": false"),
        );
        let report = load_directory(&dir).unwrap();
        assert!(report.presets[0].tags.contains(&"approximate".to_string()));
    }

    #[test]
    fn a_preset_with_no_measurement_still_loads() {
        let dir = temp_dir("nomeasure");
        write(
            &dir,
            "a.json",
            r#"{ "source": { "preset": "Bare" }, "target": { "parameters": [] } }"#,
        );
        let report = load_directory(&dir).unwrap();
        assert_eq!(report.presets[0].name, "Bare");
        assert!(report.presets[0].tags.is_empty());
        assert_eq!(report.presets[0].match_error, None);
        assert!(report.presets[0].parameters.is_empty());
    }

    #[test]
    fn a_missing_directory_is_an_error_not_an_empty_library() {
        // "the bank is empty" and "the bank is not there" are different
        // things, and a UI should be able to say which.
        let err = load_directory(std::env::temp_dir().join("preset-browser-does-not-exist"));
        assert!(err.is_err());
    }

    #[test]
    fn an_empty_directory_loads_as_an_empty_library() {
        let dir = temp_dir("empty");
        let report = load_directory(&dir).unwrap();
        assert!(report.is_empty());
        assert!(report.skipped.is_empty());
    }
}

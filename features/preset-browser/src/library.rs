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

use serde::{Deserialize, Serialize};

use crate::Preset;

/// Why a directory could not be read at all.
#[derive(Debug)]
pub enum LoadError {
    /// The directory itself could not be listed.
    Unreadable {
        path: PathBuf,
        source: std::io::Error,
    },
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

#[derive(Deserialize, Serialize)]
struct SavedPreset {
    /// Free-form prose about the preset — see [`Preset::description`].
    /// `default`/`skip_serializing_if` on the same terms as
    /// `SavedTarget::text_parameters`: an old file has no key and reads as
    /// `None`, and a preset with nothing to say writes no key at all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    source: SavedSource,
    target: SavedTarget,
    #[serde(default)]
    measurement: Option<SavedMeasurement>,
}

#[derive(Deserialize, Serialize)]
struct SavedSource {
    preset: String,
    #[serde(default)]
    plugin: Option<String>,
    #[serde(default)]
    mode: Option<String>,
}

#[derive(Deserialize, Serialize)]
struct SavedTarget {
    #[serde(default)]
    parameters: Vec<SavedParam>,
    /// Added after the 171 Pro-Q banks were written, which is why it is
    /// `default` on both ends: an old file has no key and reads as empty, and
    /// a reader that predates the key ignores it and still gets the curve.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    text_parameters: Vec<SavedTextParam>,
}

#[derive(Deserialize, Serialize)]
struct SavedParam {
    name: String,
    value: f64,
}

#[derive(Deserialize, Serialize)]
struct SavedTextParam {
    name: String,
    value: String,
}

#[derive(Deserialize, Serialize)]
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
        let match_error = self
            .measurement
            .as_ref()
            .and_then(|m| m.worst_band_ratio_error);
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
            description: self.description,
            parameters: self
                .target
                .parameters
                .into_iter()
                .map(|p| (p.name, p.value))
                .collect(),
            text_parameters: self
                .target
                .text_parameters
                .into_iter()
                .map(|p| (p.name, p.value))
                .collect(),
            match_error,
        }
    }
}

impl SavedPreset {
    fn from_preset(p: &Preset) -> Self {
        Self {
            description: p.description.clone(),
            source: SavedSource {
                preset: p.name.clone(),
                plugin: p.origin.clone(),
                mode: p.category.clone(),
            },
            target: SavedTarget {
                parameters: p
                    .parameters
                    .iter()
                    .map(|(name, value)| SavedParam {
                        name: name.clone(),
                        value: *value,
                    })
                    .collect(),
                text_parameters: p
                    .text_parameters
                    .iter()
                    .map(|(name, value)| SavedTextParam {
                        name: name.clone(),
                        value: value.clone(),
                    })
                    .collect(),
            },
            measurement: p.match_error.map(|e| SavedMeasurement {
                decay_passed: None,
                worst_band_ratio_error: Some(e),
            }),
        }
    }
}

/// Write one preset to a file, in the same shape [`load_directory`] reads.
///
/// Carries the fields the file format has: the name, its grouping, where it
/// came from, a free-form description, the numeric parameters and the string
/// ones. `author` and `tags` are browser-side derivations rather than fields
/// of the file (tags are computed from the measurement on load), so they are
/// not written and do not come back.
///
/// # Errors
///
/// Returns an error if the preset cannot be serialized or the file cannot be
/// written.
pub fn save_preset(path: impl AsRef<Path>, preset: &Preset) -> std::io::Result<()> {
    let saved = SavedPreset::from_preset(preset);
    let json = serde_json::to_string_pretty(&saved)?;
    std::fs::write(path, json)
}

/// Load every `*.json` preset in a directory (non-recursive).
///
/// Returns a report rather than a bare `Vec`, so a caller can distinguish
/// "this bank is empty" from "this bank is broken".
///
/// # Errors
///
/// Returns an error if the directory cannot be read.
pub fn load_directory(dir: impl AsRef<Path>) -> Result<LoadReport, LoadError> {
    let dir = dir.as_ref();
    let entries = std::fs::read_dir(dir).map_err(|source| LoadError::Unreadable {
        path: dir.to_path_buf(),
        source,
    })?;

    let mut report = LoadReport::default();
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("json"))
        })
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
    fn a_preset_can_carry_text_parameters_alongside_the_numbers() {
        // The EQ's per-band `name`/`notes` are user-facing strings, and a
        // preset that recalls the curve but not the labels cannot say "this
        // band is the Overheads honk".
        let dir = temp_dir("text");
        write(
            &dir,
            "named.json",
            r#"{
              "source": { "preset": "Named" },
              "target": {
                "parameters": [ { "name": "b1_freq", "value": 800.0 } ],
                "text_parameters": [
                  { "name": "b1_name", "value": "Overheads honk" },
                  { "name": "b1_notes", "value": "pulls the 800 ring out of the OHs" }
                ]
              }
            }"#,
        );
        let report = load_directory(&dir).unwrap();
        let p = &report.presets[0];
        assert_eq!(p.parameters, vec![("b1_freq".to_string(), 800.0)]);
        assert_eq!(
            p.text_parameters,
            vec![
                ("b1_name".to_string(), "Overheads honk".to_string()),
                (
                    "b1_notes".to_string(),
                    "pulls the 800 ring out of the OHs".to_string()
                ),
            ]
        );
    }

    #[test]
    fn a_saved_preset_round_trips_through_the_directory() {
        let dir = temp_dir("roundtrip");
        let written = Preset {
            name: "Overheads".to_string(),
            category: Some("Drums".to_string()),
            // `author` and `tags` are not fields of the file — see
            // `save_preset` — so a round trip cannot be asserted on them.
            author: None,
            tags: Vec::new(),
            origin: Some("FTS-EQ".to_string()),
            description: Some("Dialing order: cymbals first, then the ring.".to_string()),
            parameters: vec![("b1_freq".to_string(), 800.5), ("b1_gain".to_string(), -3.0)],
            text_parameters: vec![
                ("b1_name".to_string(), "Overheads honk".to_string()),
                ("b1_notes".to_string(), "800 ring".to_string()),
            ],
            match_error: None,
        };
        save_preset(dir.join("overheads.json"), &written).unwrap();

        let report = load_directory(&dir).unwrap();
        assert!(report.skipped.is_empty(), "{:?}", report.skipped);
        assert_eq!(report.presets, vec![written]);
    }

    #[test]
    fn a_name_with_unicode_and_a_very_long_note_survive_a_round_trip() {
        let dir = temp_dir("unicode");
        // JSON is UTF-8 and serde_json escapes what it must; the point of the
        // test is that nothing in this crate truncates, lossily converts or
        // re-encodes a label a user actually typed.
        let name = "Übergänge — 高域 “air” \u{1f941}\ttab";
        let note = "why: ".to_string() + &"the overheads ring at 800 Hz. ".repeat(400);
        let written = Preset {
            name: "Long".to_string(),
            text_parameters: vec![
                ("b1_name".to_string(), name.to_string()),
                ("b1_notes".to_string(), note.clone()),
            ],
            ..Preset::default()
        };
        save_preset(dir.join("long.json"), &written).unwrap();

        let read_back = load_directory(&dir).unwrap().presets.remove(0);
        assert_eq!(read_back.text_parameters[0].1, name);
        assert_eq!(read_back.text_parameters[1].1, note);
        assert!(note.len() > 10_000, "the note is long enough to be a test");
    }

    #[test]
    fn a_preset_with_no_names_is_written_without_the_key_at_all() {
        // Backward compatible the other way: a reader that predates
        // `text_parameters` must not meet an empty array it has no field for,
        // and a bank written by this version must diff cleanly against one
        // written by the last.
        let dir = temp_dir("nokey");
        save_preset(
            dir.join("bare.json"),
            &Preset {
                name: "Bare".to_string(),
                parameters: vec![("b1_freq".to_string(), 100.0)],
                ..Preset::default()
            },
        )
        .unwrap();
        let text = std::fs::read_to_string(dir.join("bare.json")).unwrap();
        assert!(
            !text.contains("text_parameters"),
            "no names means no key: {text}"
        );
    }

    #[test]
    fn a_legacy_nameless_preset_loads_with_no_text_parameters() {
        // The 171 translated Pro-Q 4 banks on disk have no `text_parameters`
        // key. They must keep loading exactly as they did.
        let dir = temp_dir("legacy");
        write(&dir, "legacy.json", ONE);
        let p = &load_directory(&dir).unwrap().presets[0];
        assert_eq!(p.parameters.len(), 2);
        assert!(p.text_parameters.is_empty());
    }

    #[test]
    fn a_preset_with_no_description_is_written_without_the_key_at_all() {
        // Same backward-compatibility shape as `text_parameters`: a preset
        // with nothing to say writes no key, so the 171 translated banks (and
        // every hand-written one before this field existed) diff cleanly.
        let dir = temp_dir("nodescription");
        save_preset(
            dir.join("bare.json"),
            &Preset {
                name: "Bare".to_string(),
                parameters: vec![("b1_freq".to_string(), 100.0)],
                ..Preset::default()
            },
        )
        .unwrap();
        let text = std::fs::read_to_string(dir.join("bare.json")).unwrap();
        assert!(!text.contains("description"), "no text means no key: {text}");
    }

    #[test]
    fn a_preset_written_before_description_existed_loads_with_none() {
        // `ONE` predates the field entirely — no `description` key anywhere
        // in the file. It must load, not fail to parse.
        let dir = temp_dir("predates-description");
        write(&dir, "legacy.json", ONE);
        let p = &load_directory(&dir).unwrap().presets[0];
        assert!(p.description.is_none());
    }

    #[test]
    fn a_description_survives_a_save_and_reload() {
        let dir = temp_dir("description-roundtrip");
        let written = Preset {
            name: "Overheads".to_string(),
            description: Some(
                "Dialing order: cymbal bands first, then the ring, then air.".to_string(),
            ),
            parameters: vec![("b1_freq".to_string(), 800.5)],
            ..Preset::default()
        };
        save_preset(dir.join("described.json"), &written).unwrap();
        let read_back = load_directory(&dir).unwrap().presets.remove(0);
        assert_eq!(read_back.description, written.description);
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

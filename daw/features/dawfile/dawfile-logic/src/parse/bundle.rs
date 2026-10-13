//! `.logicx` bundle reader.
//!
//! A `.logicx` file is a **directory bundle** with this structure:
//!
//! ```text
//! MyProject.logicx/
//! ├── Resources/
//! │   └── ProjectInformation.plist   # creator version, variant names
//! └── Alternatives/
//!     └── 000/                       # "alternative 0" (the active project)
//!         ├── MetaData.plist         # BPM, time sig, sample rate, key, track count
//!         └── ProjectData            # binary chunk stream (the actual session data)
//! ```
//!
//! Both plists use Apple's binary plist format (`bplist00`).

use crate::error::{LogicError, LogicResult};
use std::path::{Path, PathBuf};

/// Everything extracted from the bundle's metadata plists.
#[derive(Debug)]
pub struct BundleMeta {
    /// Creator version string, e.g. `"Logic Pro 12.0.1 (6590)"`.
    pub creator_version: String,
    /// Active variant name, e.g. `"FileDecrypt"`.
    pub variant_name: String,
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Beats per minute.
    pub bpm: f64,
    /// Time signature numerator.
    pub time_sig_numerator: u32,
    /// Time signature denominator.
    pub time_sig_denominator: u32,
    /// Root key name (e.g. `"C"`).
    pub key: String,
    /// Scale / mode (e.g. `"major"`).
    pub key_gender: String,
    /// Number of tracks declared in the metadata.
    pub track_count: u32,
}

/// Raw bytes of the active alternative's `ProjectData` binary.
pub struct BundleData {
    pub meta: BundleMeta,
    pub project_data: Vec<u8>,
}

/// Read a `.logicx` bundle from `path`, returning metadata and the raw
/// `ProjectData` bytes of the latest (highest-numbered) alternative.
pub fn read_bundle(path: &Path) -> LogicResult<BundleData> {
    if !path.is_dir() {
        return Err(LogicError::NotABundle(path.to_owned()));
    }

    // ── ProjectInformation.plist ─────────────────────────────────────────────
    let info_path = path.join("Resources").join("ProjectInformation.plist");
    let (creator_version, variant_name) = read_project_information(&info_path)?;

    // ── Pick the latest alternative (highest numeric directory name) ─────────
    let alt_dir = latest_alternative(path)?;

    // ── Alternatives/NNN/MetaData.plist ──────────────────────────────────────
    let meta_path = alt_dir.join("MetaData.plist");
    let (sample_rate, bpm, time_sig_numerator, time_sig_denominator, key, key_gender, track_count) =
        read_metadata(&meta_path)?;

    // ── Alternatives/NNN/ProjectData ─────────────────────────────────────────
    let project_data_path = alt_dir.join("ProjectData");
    let project_data = std::fs::read(&project_data_path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            LogicError::MissingFile(project_data_path.clone())
        } else {
            LogicError::Io(e)
        }
    })?;

    Ok(BundleData {
        meta: BundleMeta {
            creator_version,
            variant_name,
            sample_rate,
            bpm,
            time_sig_numerator,
            time_sig_denominator,
            key,
            key_gender,
            track_count,
        },
        project_data,
    })
}

// ── Alternative selection ─────────────────────────────────────────────────────

/// Return the path to the highest-numbered alternative directory under
/// `bundle/Alternatives/`.  Falls back to `000` if none can be parsed.
fn latest_alternative(bundle: &Path) -> LogicResult<PathBuf> {
    let alts_dir = bundle.join("Alternatives");
    if !alts_dir.is_dir() {
        return Err(LogicError::MissingFile(alts_dir));
    }

    let mut best: Option<(u32, PathBuf)> = None;
    if let Ok(entries) = std::fs::read_dir(&alts_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if !p.is_dir() {
                continue;
            }
            if let Some(name) = p.file_name().and_then(|n| n.to_str())
                && let Ok(n) = name.parse::<u32>()
            {
                // Only consider directories that actually have a ProjectData file.
                if p.join("ProjectData").exists() && best.as_ref().is_none_or(|(b, _)| n > *b) {
                    best = Some((n, p));
                }
            }
        }
    }

    best.map(|(_, p)| p)
        .or_else(|| {
            let fallback = alts_dir.join("000");
            fallback.is_dir().then_some(fallback)
        })
        .ok_or_else(|| LogicError::MissingFile(alts_dir.join("000")))
}

// ── plist helpers ─────────────────────────────────────────────────────────────

fn plist_err(path: &Path, e: plist::Error) -> LogicError {
    LogicError::Plist {
        path: path.to_owned(),
        source: e,
    }
}

/// Read `ProjectInformation.plist` → (creator_version, variant_name).
fn read_project_information(path: &Path) -> LogicResult<(String, String)> {
    ensure_exists(path)?;
    let value: plist::Value = plist::from_file(path).map_err(|e| plist_err(path, e))?;

    let dict = value
        .as_dictionary()
        .ok_or_else(|| LogicError::PlistStructure {
            path: path.to_owned(),
            reason: "root value is not a dictionary".into(),
        })?;

    let creator_version = dict
        .get("LastSavedFrom")
        .and_then(|v| v.as_string())
        .unwrap_or("unknown")
        .to_owned();

    // VariantNames is a dict keyed by alternative index; key "0" is the active one.
    let variant_name = dict
        .get("VariantNames")
        .and_then(|v| v.as_dictionary())
        .and_then(|d| d.get("0"))
        .and_then(|v| v.as_string())
        .unwrap_or("")
        .to_owned();

    Ok((creator_version, variant_name))
}

/// Read `MetaData.plist` → (sample_rate, bpm, num, den, key, gender, track_count).
fn read_metadata(path: &Path) -> LogicResult<(u32, f64, u32, u32, String, String, u32)> {
    ensure_exists(path)?;
    let value: plist::Value = plist::from_file(path).map_err(|e| plist_err(path, e))?;

    let dict = value
        .as_dictionary()
        .ok_or_else(|| LogicError::PlistStructure {
            path: path.to_owned(),
            reason: "root value is not a dictionary".into(),
        })?;

    let sample_rate = dict
        .get("SampleRate")
        .and_then(|v| v.as_unsigned_integer())
        .unwrap_or(48000) as u32;

    let bpm = dict
        .get("BeatsPerMinute")
        .and_then(|v| v.as_real())
        .unwrap_or(120.0);

    let time_sig_numerator = dict
        .get("SongSignatureNumerator")
        .and_then(|v| v.as_unsigned_integer())
        .unwrap_or(4) as u32;

    let time_sig_denominator = dict
        .get("SongSignatureDenominator")
        .and_then(|v| v.as_unsigned_integer())
        .unwrap_or(4) as u32;

    let key = dict
        .get("SongKey")
        .and_then(|v| v.as_string())
        .unwrap_or("C")
        .to_owned();

    let key_gender = dict
        .get("SongGenderKey")
        .and_then(|v| v.as_string())
        .unwrap_or("major")
        .to_owned();

    let track_count = dict
        .get("NumberOfTracks")
        .and_then(|v| v.as_unsigned_integer())
        .unwrap_or(0) as u32;

    Ok((
        sample_rate,
        bpm,
        time_sig_numerator,
        time_sig_denominator,
        key,
        key_gender,
        track_count,
    ))
}

fn ensure_exists(path: &Path) -> LogicResult<()> {
    if !path.exists() {
        Err(LogicError::MissingFile(path.to_owned()))
    } else {
        Ok(())
    }
}

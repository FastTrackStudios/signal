//! Sample file discovery and lookup.
//!
//! The sample map builds an index over all sample files in a library root
//! directory, then provides O(1) lookup by (section, mic, articulation,
//! dynamic, note, rr_index, direction).
//!
//! # File naming convention
//!
//! CS-family libraries extracted from NKX archives use this flat naming pattern:
//!
//! ```text
//! {Section}_{Artic}_{Mic}_{Dyn}_{NoteOct}[_{Dir}][_{RR}].wav
//! ```
//!
//! Examples:
//! - `1v_Vibsus_Mix_ppp_G2.wav`            (single RR, no direction)
//! - `1v_Leg_Mix_p_G2_up_RR1.wav`          (directional legato, RR 1)
//! - `1v_Staccato_Mix_pp_G2_RR3.wav`       (short note, RR 3)
//! - `Ce_NVLeg_Main_mf_C2_down_RR07.wav`   (padded RR index)
//!
//! The scanner is tolerant of minor formatting variations (padded/unpadded RR
//! numbers, `RR` vs `rr` prefix, missing RR suffix for single-RR articulations).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::{
    SamplerError,
    midi::{nearest_grid_note, note_name_to_midi},
    spec::LibrarySpec,
};

// ── Sample key ────────────────────────────────────────────────────────────────

/// Unique lookup key for one sample file.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SampleKey {
    /// Section id: `"1v"`, `"Ce"`, `"2tpt"`, etc.
    pub section: String,
    /// Articulation id: `"Vibsus"`, `"Leg"`, `"Staccato"`, etc.
    pub articulation: String,
    /// Mic id: `"Mix"`, `"Main"`, `"Room"`, etc.
    pub mic: String,
    /// Dynamic label: `"ppp"`, `"p"`, `"mf"`, `"ff"`, `"fff"`, etc.
    pub dynamic: String,
    /// MIDI note number (the sampled root note).
    pub note: u8,
    /// Direction for legato transitions: `"up"`, `"down"`, or `""` (none).
    pub direction: String,
    /// Round-robin index (0-based).
    pub rr: usize,
}

// ── Sample map ────────────────────────────────────────────────────────────────

/// In-memory index: `SampleKey → absolute sample path`.
pub struct SampleMap {
    /// Primary index.
    map: HashMap<SampleKey, PathBuf>,
    /// Total files indexed.
    total: usize,
}

impl SampleMap {
    /// Build an empty map. Useful in tests.
    pub fn empty() -> Self {
        Self {
            map: HashMap::new(),
            total: 0,
        }
    }

    /// Build a `SampleMap` from an iterator of relative paths — typically
    /// the entry paths of a `.signalpack`. Each path is parsed by
    /// [`parse_sample_stem`] and indexed identically to a filesystem scan.
    /// Used by convention-mode libraries (e.g. Keyscape) that ship as packs
    /// with no on-disk source.
    pub fn from_paths<I, P>(paths: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        let mut map = HashMap::new();
        for raw in paths {
            let path: PathBuf = raw.into();
            let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
                continue;
            };
            if !is_supported_sample_ext(ext) {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if let Some(key) = parse_sample_stem(stem) {
                insert_sample(&mut map, key, path);
            }
        }
        let total = map.len();
        Self { map, total }
    }

    /// Scan `root_dir` and build a sample map.
    ///
    /// Expects sample files either directly in `root_dir` (flat layout) or
    /// nested in subdirectories (organised layout). `.wav` and `.flac` files
    /// are parsed using [`parse_sample_stem`].
    pub fn scan(root_dir: &Path) -> Result<Self, SamplerError> {
        let mut map = HashMap::new();
        scan_dir(root_dir, &mut map)?;
        let total = map.len();
        Ok(Self { map, total })
    }

    /// Total number of sample files indexed.
    pub fn total(&self) -> usize {
        self.total
    }

    /// Paths for all indexed samples.
    pub fn paths(&self) -> impl Iterator<Item = &PathBuf> {
        self.map.values()
    }

    /// Look up the exact path for a sample key.
    pub fn get(&self, key: &SampleKey) -> Option<&PathBuf> {
        self.map.get(key)
    }

    /// Resolve a playback lookup to a sample path, performing pitch-rounding
    /// to the nearest sampled note in the spec's `note_grid`.
    ///
    /// If `target_note` is not directly sampled, the nearest grid note is
    /// used and the engine is expected to transpose the sample at playback.
    pub fn resolve(
        &self,
        spec: &LibrarySpec,
        section_id: &str,
        articulation_id: &str,
        mic_id: &str,
        dynamic: &str,
        target_note: u8,
        direction: &str,
        rr: usize,
    ) -> Option<(PathBuf, u8 /* sampled_note */)> {
        // Find the section to get the note grid.
        let section = spec.section(section_id)?;
        let lowest = note_name_to_midi(&section.lowest_note).ok()?;
        let highest = note_name_to_midi(&section.highest_note).ok()?;

        let sampled = if section.note_grid.is_empty() {
            // No grid — try exact note first, then walk outward.
            target_note.clamp(lowest, highest)
        } else {
            nearest_grid_note(target_note, &section.note_grid, lowest, highest)
        };

        // Build the candidate token list: primary id + any aliases from the spec.
        let aliases = spec
            .articulation(articulation_id)
            .map(|a| a.aliases.as_slice())
            .unwrap_or(&[]);

        let mut key = SampleKey {
            section: section_id.to_string(),
            articulation: articulation_id.to_string(),
            mic: mic_id.to_string(),
            dynamic: dynamic.to_string(),
            note: sampled,
            direction: direction.to_string(),
            rr,
        };

        // Try primary token first, then each alias in order.
        if let Some(p) = self.map.get(&key) {
            return Some((p.clone(), sampled));
        }
        if let Some(p) = self.nearest_dynamic_path(&key) {
            return Some((p.clone(), sampled));
        }
        for alias in aliases {
            key.articulation = alias.clone();
            if let Some(p) = self.map.get(&key) {
                return Some((p.clone(), sampled));
            }
            if let Some(p) = self.nearest_dynamic_path(&key) {
                return Some((p.clone(), sampled));
            }
        }
        None
    }

    fn nearest_dynamic_path(&self, key: &SampleKey) -> Option<&PathBuf> {
        let wanted = key.dynamic.parse::<i16>().ok()?;
        let direction =
            if key.direction.is_empty() && key.articulation.to_ascii_lowercase().contains("lacr") {
                None
            } else {
                Some(key.direction.as_str())
            };
        self.map
            .iter()
            .filter(|(candidate, _)| {
                candidate.section == key.section
                    && candidate.articulation == key.articulation
                    && candidate.mic == key.mic
                    && candidate.note == key.note
                    && candidate.rr == key.rr
                    && direction.map_or(true, |direction| candidate.direction == direction)
            })
            .filter_map(|(candidate, path)| {
                let dynamic = candidate.dynamic.parse::<i16>().ok()?;
                Some(((dynamic - wanted).abs(), path))
            })
            .min_by_key(|(distance, _)| *distance)
            .map(|(_, path)| path)
    }

    /// Iterate all indexed sample keys.
    pub fn iter(&self) -> impl Iterator<Item = (&SampleKey, &PathBuf)> {
        self.map.iter()
    }

    /// All (section_id, articulation_id) pairs present in the map.
    pub fn articulations_present(&self) -> Vec<(String, String)> {
        let mut pairs: Vec<(String, String)> = self
            .map
            .keys()
            .map(|k| (k.section.clone(), k.articulation.clone()))
            .collect();
        pairs.sort();
        pairs.dedup();
        pairs
    }
}

// ── Directory scanner ─────────────────────────────────────────────────────────

fn scan_dir(dir: &Path, map: &mut HashMap<SampleKey, PathBuf>) -> Result<(), SamplerError> {
    for entry in std::fs::read_dir(dir).map_err(SamplerError::Io)? {
        let entry = entry.map_err(SamplerError::Io)?;
        let path = entry.path();
        if path.is_dir() {
            scan_dir(&path, map)?;
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if is_supported_sample_ext(ext) {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if let Some(key) = parse_sample_stem(stem) {
                        insert_sample(map, key, path);
                    }
                }
            }
        }
    }
    Ok(())
}

fn insert_sample(map: &mut HashMap<SampleKey, PathBuf>, key: SampleKey, path: PathBuf) {
    if let Some(existing) = map.get(&key) {
        if prefer_existing_keyscape_sample(existing, &path) {
            return;
        }
    }
    map.insert(key, path);
}

fn prefer_existing_keyscape_sample(existing: &Path, candidate: &Path) -> bool {
    let existing_stem = existing
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    let candidate_stem = candidate
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    let existing_dynamic = existing_stem.split_whitespace().nth(3).unwrap_or_default();
    let candidate_dynamic = candidate_stem.split_whitespace().nth(3).unwrap_or_default();

    match (
        existing_dynamic.contains('_'),
        candidate_dynamic.contains('_'),
    ) {
        (false, true) => true,
        (true, false) => false,
        _ => true,
    }
}

fn is_supported_sample_ext(ext: &str) -> bool {
    ext.eq_ignore_ascii_case("wav") || ext.eq_ignore_ascii_case("flac")
}

// ── Filename parser ───────────────────────────────────────────────────────────

/// Parse a WAV filename stem into a `SampleKey`.
///
/// Expected patterns (underscore-separated):
/// - Standard:    `{Section}_{Artic}_{Mic}_{Dyn}_{Note}[_{Dir}][_{RR}]`
/// - Dir-legato:  `{Section}_{Artic}_{Mic}_{Dyn}_{Dir}_{Note}_{RR}`
///   (Leg / NVLeg / Port have direction **before** the note in CSS filenames)
///
/// Returns `None` if the stem cannot be parsed (non-CS file, etc.).
pub fn parse_wav_stem(stem: &str) -> Option<SampleKey> {
    parse_sample_stem(stem)
}

pub fn parse_sample_stem(stem: &str) -> Option<SampleKey> {
    parse_signal_stem(stem)
        .or_else(|| parse_keyscape_c7_stem(stem))
        .or_else(|| parse_keyscape_wurlitzer_stem(stem))
        .or_else(|| parse_keyscape_stem(stem))
        .or_else(|| parse_keyscape_loose_stem(stem))
}

fn parse_signal_stem(stem: &str) -> Option<SampleKey> {
    let parts: Vec<&str> = stem.split('_').collect();
    if parts.len() < 5 {
        return None;
    }

    let section = parts[0].to_string();
    let articulation = parts[1].to_string();
    let mic = parts[2].to_string();
    let dynamic = parts[3].to_string();

    // Two naming variants exist:
    //   Standard:   {sec}_{artic}_{mic}_{dyn}_{note}[_{dir}][_{rr}]
    //   Directional legato: {sec}_{artic}_{mic}_{dyn}_{dir}_{note}_{rr}
    //     (Leg, NVLeg, Port have direction BEFORE the note in the filename)
    //
    // Discriminate by checking whether parts[4] is a direction word or a note name.
    let mut direction = String::new();
    let mut rr: usize = 0;
    let note_str;
    let remaining_start;

    let p4_lower = parts[4].to_ascii_lowercase();
    if p4_lower == "up" || p4_lower == "down" {
        // Directional-legato layout: dir at [4], note at [5].
        if parts.len() < 6 {
            return None;
        }
        direction = p4_lower;
        note_str = parts[5];
        remaining_start = 6;
    } else {
        // Standard layout: note at [4].
        note_str = parts[4];
        remaining_start = 5;
    }

    // Parse MIDI note (e.g. "G2", "C#4", "A#1").
    let note = note_name_to_midi(note_str).ok()?;

    // Remaining tokens: optional direction (standard layout) + optional RR.
    for tok in &parts[remaining_start..] {
        let lower = tok.to_ascii_lowercase();
        if lower == "up" || lower == "down" {
            direction = lower;
        } else if let Some(rr_str) = lower.strip_prefix("rr") {
            rr = rr_str.parse::<usize>().unwrap_or(1).saturating_sub(1); // 1-based → 0-based
        } else if let Ok(n) = lower.parse::<usize>() {
            // Bare number treated as 1-based RR index.
            rr = n.saturating_sub(1);
        }
    }

    Some(SampleKey {
        section,
        articulation,
        mic,
        dynamic,
        note,
        direction,
        rr,
    })
}

/// Parse flat Keyscape-style stems:
///
/// ```text
/// RR01 lacrm 60 96
/// RR03 lacr 84 109 relm
/// ```
fn parse_keyscape_stem(stem: &str) -> Option<SampleKey> {
    let parts: Vec<&str> = stem.split_whitespace().collect();
    if parts.len() < 4 {
        return None;
    }

    let rr = parts[0]
        .strip_prefix("RR")
        .and_then(|s| s.parse::<usize>().ok())?
        .saturating_sub(1);
    let articulation = parts[1].to_ascii_lowercase();
    let note = parts[2].parse::<u8>().ok()?;
    let dynamic = parts[3].split('_').next().unwrap_or(parts[3]).to_string();

    let mut direction = String::new();
    for tok in &parts[4..] {
        let lower = tok.to_ascii_lowercase();
        if lower.starts_with("rel") {
            direction = lower;
            break;
        }
    }

    Some(SampleKey {
        section: "main".to_string(),
        articulation,
        mic: "Main".to_string(),
        dynamic,
        note,
        direction,
        rr,
    })
}

fn parse_keyscape_c7_stem(stem: &str) -> Option<SampleKey> {
    if stem.starts_with("RR") && stem.get(4..5) == Some("_") {
        let (rr_part, rest) = stem.split_once('_')?;
        let rr = rr_part
            .strip_prefix("RR")
            .and_then(|s| s.parse::<usize>().ok())?
            .saturating_sub(1);
        let (artic_token, note_dyn) = rest.rsplit_once('_')?;
        let (note, dynamic) = parse_keyscape_note_dynamic(note_dyn)?;
        let articulation = if artic_token.contains("LACPPU") {
            "lacppu"
        } else if artic_token.contains("LACPPD") {
            "lacppd"
        } else {
            return None;
        };

        return Some(SampleKey {
            section: "main".to_string(),
            articulation: articulation.to_string(),
            mic: "Main".to_string(),
            dynamic,
            note,
            direction: String::new(),
            rr,
        });
    }

    let parts: Vec<&str> = stem.split_whitespace().collect();
    if parts.len() == 4 && parts[1].eq_ignore_ascii_case("grndpno") {
        let rr = parts[0]
            .strip_prefix("RR")
            .and_then(|s| s.parse::<usize>().ok())?
            .saturating_sub(1);
        let dynamic = parts[2].to_ascii_lowercase();

        return Some(SampleKey {
            section: "main".to_string(),
            articulation: "grndpno".to_string(),
            mic: "Main".to_string(),
            dynamic,
            note: 60,
            direction: String::new(),
            rr,
        });
    }

    if parts.len() >= 4
        && parts[1].eq_ignore_ascii_case("LACP")
        && parts[2].eq_ignore_ascii_case("Rel")
    {
        let rr = parts[0]
            .strip_prefix("RR")
            .and_then(|s| s.parse::<usize>().ok())?
            .saturating_sub(1);
        let (note, dynamic) = parse_keyscape_note_dynamic(parts[3].rsplit_once('_')?.1)?;

        return Some(SampleKey {
            section: "main".to_string(),
            articulation: "lacprel".to_string(),
            mic: "Main".to_string(),
            dynamic,
            note,
            direction: "rel".to_string(),
            rr,
        });
    }

    None
}

fn parse_keyscape_note_dynamic(note_dyn: &str) -> Option<(u8, String)> {
    let (note, dynamic) = note_dyn.split_once('-')?;
    Some((note.parse().ok()?, dynamic.to_string()))
}

fn parse_keyscape_wurlitzer_stem(stem: &str) -> Option<SampleKey> {
    parse_keyscape_wurlitzer_200a_stem(stem).or_else(|| parse_keyscape_wurlitzer_140b_stem(stem))
}

fn parse_keyscape_wurlitzer_200a_stem(stem: &str) -> Option<SampleKey> {
    let parts: Vec<&str> = stem.split_whitespace().collect();
    if parts.len() < 4 || !parts[0].eq_ignore_ascii_case("NMWurl") {
        return None;
    }

    let note = parts[1].parse::<u8>().ok()?;
    if !parts[2].eq_ignore_ascii_case("a") {
        return None;
    }

    let release = parts
        .get(4)
        .is_some_and(|token| token.trim_end_matches("-o").eq_ignore_ascii_case("Rls"));
    let dynamic = parts[3].trim_end_matches("-o").to_string();

    Some(SampleKey {
        section: "main".to_string(),
        articulation: if release { "nmwurlrel" } else { "nmwurl" }.to_string(),
        mic: "Main".to_string(),
        dynamic,
        note,
        direction: if release { "rel" } else { "" }.to_string(),
        rr: 0,
    })
}

fn parse_keyscape_wurlitzer_140b_stem(stem: &str) -> Option<SampleKey> {
    if let Some(key) = parse_keyscape_wurlitzer_140b_main_or_pedal(stem) {
        return Some(key);
    }

    let parts: Vec<&str> = stem.split('-').collect();
    if parts.len() < 4 {
        return None;
    }

    let rr = parts[0]
        .strip_prefix("RR")
        .and_then(|s| s.parse::<usize>().ok())?
        .saturating_sub(1);
    let note = parts[1].parse::<u8>().ok()?;
    let dynamic = parts[2].to_string();
    let tail = parts[3].to_ascii_lowercase();
    let (articulation, direction) = if tail.starts_with("relfastr7") {
        ("wurl140brelfastr7", "rel")
    } else if tail.starts_with("relfast") {
        ("wurl140brelfast", "rel")
    } else if tail.starts_with("mecsus") {
        ("wurl140bmecsus", "")
    } else if tail.starts_with("mechrel") {
        ("wurl140bmechrel", "rel")
    } else {
        return None;
    };

    Some(SampleKey {
        section: "main".to_string(),
        articulation: articulation.to_string(),
        mic: "Main".to_string(),
        dynamic,
        note,
        direction: direction.to_string(),
        rr,
    })
}

fn parse_keyscape_wurlitzer_140b_main_or_pedal(stem: &str) -> Option<SampleKey> {
    let parts: Vec<&str> = stem.split_whitespace().collect();
    let rr = parts
        .first()?
        .strip_prefix("RR")
        .and_then(|s| s.parse::<usize>().ok())?
        .saturating_sub(1);

    if parts.len() == 3 {
        return Some(SampleKey {
            section: "main".to_string(),
            articulation: "wurl140b".to_string(),
            mic: "Main".to_string(),
            note: parts[1].parse().ok()?,
            dynamic: parts[2].to_string(),
            direction: String::new(),
            rr,
        });
    }

    if parts.len() == 4
        && parts[1].eq_ignore_ascii_case("140B")
        && parts[2].eq_ignore_ascii_case("PN")
    {
        return Some(SampleKey {
            section: "main".to_string(),
            articulation: "wurl140bpn".to_string(),
            mic: "Main".to_string(),
            note: 60,
            dynamic: parts[3].to_ascii_lowercase(),
            direction: String::new(),
            rr,
        });
    }

    None
}

fn parse_keyscape_loose_stem(stem: &str) -> Option<SampleKey> {
    let tokens = loose_tokens(stem);
    if tokens.len() < 2 {
        return None;
    }

    let rr = loose_rr(&tokens);
    let numeric = tokens
        .iter()
        .enumerate()
        .filter_map(|(idx, token)| token.parse::<u16>().ok().map(|value| (idx, value)))
        .collect::<Vec<_>>();

    let note_idx = numeric
        .iter()
        .find(|(_, value)| *value <= 127)
        .map(|(idx, _)| *idx)?;
    let note = tokens[note_idx].parse::<u8>().ok()?;
    // Velocity-layer tokens like `v01`..`v19` (Keyscape Classic-style)
    // encode the dynamic separately from the note. Map them onto a
    // velocity-scale dynamic label so different velocity layers don't
    // collapse onto the same (artic, note, dyn) key and overwrite each
    // other.
    let v_dyn = tokens.iter().enumerate().find_map(|(idx, token)| {
        if idx == note_idx {
            return None;
        }
        let rest = token.strip_prefix('v')?;
        let n: u16 = rest.parse().ok()?;
        // Map v01..v19 → roughly equal spacing across 1..=127 so the
        // dynamic-picker (closest by velocity) works as expected.
        // v01 → 7, v10 → 67, v19 → 126.
        let mapped = ((n as f32 / 19.0) * 127.0).round() as u16;
        Some(mapped.clamp(1, 127).to_string())
    });
    let dynamic = v_dyn.unwrap_or_else(|| {
        numeric
            .iter()
            .rev()
            .find(|(idx, value)| *idx != note_idx && *value <= 127)
            .map(|(_, value)| value.to_string())
            .unwrap_or_else(|| "127".to_string())
    });

    let articulation = loose_articulation(&tokens, note_idx)?;
    let direction = if articulation.contains("rel") || tokens.iter().any(|token| token == "rel") {
        "rel".to_string()
    } else {
        String::new()
    };

    Some(SampleKey {
        section: "main".to_string(),
        articulation,
        mic: "Main".to_string(),
        dynamic,
        note,
        direction,
        rr,
    })
}

fn loose_tokens(stem: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut previous_was_digit = false;
    for ch in stem.chars() {
        if ch.is_ascii_alphanumeric() {
            if previous_was_digit && ch.is_ascii_alphabetic() && !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            current.push(ch.to_ascii_lowercase());
            previous_was_digit = ch.is_ascii_digit();
        } else if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
            previous_was_digit = false;
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn loose_rr(tokens: &[String]) -> usize {
    tokens
        .iter()
        .find_map(|token| {
            token
                .strip_prefix("rr")
                .or_else(|| token.strip_prefix('r'))
                .and_then(|rest| rest.parse::<usize>().ok())
        })
        .unwrap_or(1)
        .saturating_sub(1)
}

fn loose_articulation(tokens: &[String], note_idx: usize) -> Option<String> {
    let mut parts = Vec::new();
    for token in &tokens[..note_idx] {
        if token.starts_with("rr") || token.starts_with("sl") || token == "nr" {
            continue;
        }
        if token.parse::<u16>().is_ok() {
            continue;
        }
        parts.push(token.as_str());
    }

    if parts.is_empty() {
        for token in &tokens[note_idx + 1..] {
            if token.starts_with('r') || token.starts_with('v') || token.parse::<u16>().is_ok() {
                continue;
            }
            parts.push(token.as_str());
        }
    }

    let articulation = parts.join("");
    (!articulation.is_empty()).then_some(articulation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic() {
        let key = parse_wav_stem("1v_Vibsus_Mix_ppp_G2").unwrap();
        assert_eq!(key.section, "1v");
        assert_eq!(key.articulation, "Vibsus");
        assert_eq!(key.mic, "Mix");
        assert_eq!(key.dynamic, "ppp");
        assert_eq!(key.note, 43); // G2
        assert_eq!(key.direction, "");
        assert_eq!(key.rr, 0);
    }

    #[test]
    fn parse_legato_with_direction_and_rr() {
        let key = parse_wav_stem("1v_Leg_Mix_p_G2_up_RR1").unwrap();
        assert_eq!(key.articulation, "Leg");
        assert_eq!(key.direction, "up");
        assert_eq!(key.rr, 0); // RR1 → index 0
    }

    #[test]
    fn parse_classic_clr_r10_velocity() {
        let key = parse_sample_stem("RR01_SL01 CLR r10_60 v01").unwrap();
        eprintln!("v01 → {key:?}");
        assert_eq!(key.articulation, "clrr10");
        assert_eq!(key.note, 60);
        let key10 = parse_sample_stem("RR01_SL01 CLR r10_60 v10").unwrap();
        eprintln!("v10 → {key10:?}");
        let key19 = parse_sample_stem("RR01_SL01 CLR r10_60 v19").unwrap();
        eprintln!("v19 → {key19:?}");
        assert_ne!(key.dynamic, key10.dynamic);
        assert_ne!(key10.dynamic, key19.dynamic);
    }

    #[test]
    fn parse_rr3() {
        let key = parse_wav_stem("Ce_Staccato_Main_pp_C2_RR3").unwrap();
        assert_eq!(key.rr, 2); // RR3 → index 2
        assert_eq!(key.note, 36); // C2
    }

    #[test]
    fn parse_keyscape_main_sample() {
        let key = parse_sample_stem("RR04 lacrm 60 126_2").unwrap();
        assert_eq!(key.section, "main");
        assert_eq!(key.articulation, "lacrm");
        assert_eq!(key.mic, "Main");
        assert_eq!(key.dynamic, "126");
        assert_eq!(key.note, 60);
        assert_eq!(key.rr, 3);
    }

    #[test]
    fn parse_keyscape_c7_main_sample() {
        let key = parse_sample_stem("RR01_SL01LACPPUr09_60-64").unwrap();
        assert_eq!(key.section, "main");
        assert_eq!(key.articulation, "lacppu");
        assert_eq!(key.mic, "Main");
        assert_eq!(key.dynamic, "64");
        assert_eq!(key.note, 60);
        assert_eq!(key.rr, 0);

        let key = parse_sample_stem("RR01_SL02LACPPDr09_60-64").unwrap();
        assert_eq!(key.articulation, "lacppd");
    }

    #[test]
    fn parse_keyscape_c7_release_sample() {
        let key = parse_sample_stem("RR01 LACP Rel r08_60-64").unwrap();
        assert_eq!(key.articulation, "lacprel");
        assert_eq!(key.dynamic, "64");
        assert_eq!(key.note, 60);
        assert_eq!(key.direction, "rel");
    }

    #[test]
    fn parse_keyscape_wurlitzer_200a_samples() {
        let key = parse_sample_stem("NMWurl 33 a 0-14-o").unwrap();
        assert_eq!(key.articulation, "nmwurl");
        assert_eq!(key.dynamic, "0-14");
        assert_eq!(key.note, 33);
        assert_eq!(key.direction, "");

        let key = parse_sample_stem("NMWurl 33 a 0-14 Rls-o").unwrap();
        assert_eq!(key.articulation, "nmwurlrel");
        assert_eq!(key.dynamic, "0-14");
        assert_eq!(key.note, 33);
        assert_eq!(key.direction, "rel");
    }

    #[test]
    fn parse_keyscape_wurlitzer_140b_samples() {
        let key = parse_sample_stem("RR01 33 20").unwrap();
        assert_eq!(key.articulation, "wurl140b");
        assert_eq!(key.dynamic, "20");
        assert_eq!(key.note, 33);
        assert_eq!(key.rr, 0);

        let key = parse_sample_stem("RR05-52-75-RelFastr7").unwrap();
        assert_eq!(key.articulation, "wurl140brelfastr7");
        assert_eq!(key.dynamic, "75");
        assert_eq!(key.note, 52);
        assert_eq!(key.direction, "rel");
        assert_eq!(key.rr, 4);

        let key = parse_sample_stem("RR05-53-109-MecSus4").unwrap();
        assert_eq!(key.articulation, "wurl140bmecsus");
        assert_eq!(key.dynamic, "109");

        let key = parse_sample_stem("RR01 140B PN DI_0r02").unwrap();
        assert_eq!(key.articulation, "wurl140bpn");
        assert_eq!(key.dynamic, "di_0r02");
    }

    #[test]
    fn parse_keyscape_loose_samples() {
        let key = parse_sample_stem("Classic Toy Piano_60-109 r3").unwrap();
        assert_eq!(key.articulation, "classictoypiano");
        assert_eq!(key.note, 60);
        assert_eq!(key.dynamic, "109");
        assert_eq!(key.rr, 2);

        let key = parse_sample_stem("cat chime 55 rr01A_0001 rm").unwrap();
        assert_eq!(key.articulation, "catchime");
        assert_eq!(key.note, 55);
        assert_eq!(key.rr, 0);

        let key = parse_sample_stem("RR01_SL01 DulciRelr02_100-F").unwrap();
        assert_eq!(key.articulation, "dulcirelr02");
        assert_eq!(key.direction, "rel");
        assert_eq!(key.note, 100);
    }

    #[test]
    fn nearest_numeric_dynamic_uses_available_rr_layer() {
        let mut map = HashMap::new();
        let mut key = SampleKey {
            section: "main".to_string(),
            articulation: "lacrm".to_string(),
            mic: "Main".to_string(),
            dynamic: "84".to_string(),
            note: 66,
            direction: String::new(),
            rr: 1,
        };
        map.insert(key.clone(), PathBuf::from("RR02 lacrm 66 84.flac"));
        key.dynamic = "126".to_string();
        map.insert(key.clone(), PathBuf::from("RR02 lacrm 66 126.flac"));

        key.dynamic = "106".to_string();
        let sample_map = SampleMap { map, total: 2 };
        assert_eq!(
            sample_map.nearest_dynamic_path(&key),
            Some(&PathBuf::from("RR02 lacrm 66 126.flac"))
        );
    }

    #[test]
    fn parse_directional_legato() {
        // CSS legato: direction comes BEFORE note  →  {sec}_{artic}_{mic}_{dyn}_{dir}_{note}_{rr}
        let key = parse_wav_stem("1v_Leg_Mix_ff_up_A3_12").unwrap();
        assert_eq!(key.articulation, "Leg");
        assert_eq!(key.direction, "up");
        assert_eq!(key.note, 57); // A3
        assert_eq!(key.rr, 11); // 12 → index 11

        let key = parse_wav_stem("1v_Leg_Mix_mf_down_B5_3").unwrap();
        assert_eq!(key.direction, "down");
        assert_eq!(key.note, 83); // B5
        assert_eq!(key.rr, 2);

        // NVLeg same layout
        let key = parse_wav_stem("1v_NVLeg_Mix_mf_up_A2_1").unwrap();
        assert_eq!(key.articulation, "NVLeg");
        assert_eq!(key.direction, "up");
        assert_eq!(key.note, 45); // A2
        assert_eq!(key.rr, 0);
    }

    #[test]
    fn parse_legzero_standard_layout() {
        // Legzero has no direction — stays on the standard layout
        let key = parse_wav_stem("1v_NVLegzero_Mix_ff_F3_3").unwrap();
        assert_eq!(key.articulation, "NVLegzero");
        assert_eq!(key.direction, "");
        assert_eq!(key.note, 53); // F3
        assert_eq!(key.rr, 2); // 3 → index 2
    }

    #[test]
    fn parse_too_short_returns_none() {
        assert!(parse_wav_stem("random_file").is_none());
    }
}

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
    midi::{nearest_grid_note, note_name_to_midi},
    spec::LibrarySpec,
    SamplerError,
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

// ── Sample query ──────────────────────────────────────────────────────────────

/// Playback-lookup parameters for [`SampleMap::resolve`].
#[derive(Debug, Clone, Copy)]
pub struct SampleQuery<'a> {
    pub section_id: &'a str,
    pub articulation_id: &'a str,
    pub mic_id: &'a str,
    pub dynamic: &'a str,
    pub target_note: u8,
    pub direction: &'a str,
    pub rr: usize,
}

// ── Sample map ────────────────────────────────────────────────────────────────

/// In-memory index: `SampleKey → absolute sample path`.
#[derive(Clone)]
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
        query: &SampleQuery<'_>,
    ) -> Option<(PathBuf, u8 /* sampled_note */)> {
        let &SampleQuery {
            section_id,
            articulation_id,
            mic_id,
            dynamic,
            target_note,
            direction,
            rr,
        } = query;

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
        // Rank by dynamic distance FIRST, then round-robin distance. The RR
        // index is decorative variation; the velocity layer decides the
        // sample's loudness AND length. Filtering by exact RR (as an earlier
        // version did) is wrong when a library records different RR counts per
        // dynamic — e.g. Keyscape LA Custom ships 4 RRs for its mid layers but
        // only 1 (rr0) for the long high-velocity sustains. With an exact-RR
        // filter, note-ons whose RR counter lands on 1-3 miss the requested
        // dynamic entirely and fall back to a *shorter* neighbouring layer,
        // so the note dies right after the attack. Keeping the dynamic and
        // relaxing the RR fixes that while leaving properly-RR'd dynamics
        // (which exact-match before this fallback runs) untouched.
        self.map
            .iter()
            .filter(|(candidate, _)| {
                candidate.section == key.section
                    && candidate.articulation == key.articulation
                    && candidate.mic == key.mic
                    && candidate.note == key.note
                    && direction.is_none_or(|direction| candidate.direction == direction)
            })
            .filter_map(|(candidate, path)| {
                let dynamic = candidate.dynamic.parse::<i16>().ok()?;
                let rr_distance = (candidate.rr as i32 - key.rr as i32).unsigned_abs();
                Some(((dynamic - wanted).abs(), rr_distance, path))
            })
            .min_by_key(|(dyn_distance, rr_distance, _)| (*dyn_distance, *rr_distance))
            .map(|(_, _, path)| path)
    }

    /// Iterate all indexed sample keys.
    pub fn iter(&self) -> impl Iterator<Item = (&SampleKey, &PathBuf)> {
        self.map.iter()
    }

    /// All distinct `direction` (blend-layer) values present for this
    /// (section, articulation, mic, note, dynamic), sorted. E.g.
    /// `["rel", "relm", "relsl"]` for a release, or `["", "2"]` for a body
    /// that ships a second hard-hit layer. Empty when nothing matches (the
    /// caller then falls back to a single nearest-match voice).
    pub fn layer_directions(
        &self,
        section: &str,
        articulation: &str,
        mic: &str,
        note: u8,
        dynamic: &str,
    ) -> Vec<String> {
        let mut dirs: Vec<String> = self
            .map
            .keys()
            .filter(|k| {
                k.section == section
                    && k.articulation == articulation
                    && k.mic == mic
                    && k.note == note
                    && k.dynamic == dynamic
            })
            .map(|k| k.direction.clone())
            .collect();
        dirs.sort();
        dirs.dedup();
        dirs
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
    // The dynamic may carry a body-layer suffix — `126_2` is a SECOND body
    // layer that blends with `126` at the hardest hits. Keep the layer as the
    // `direction` discriminator so the two files don't collide on one key (they
    // did, silently dropping a layer). Base body → direction "".
    let (dynamic, mut direction) = match parts[3].split_once('_') {
        Some((base, layer)) => (base.to_string(), layer.to_string()),
        None => (parts[3].to_string(), String::new()),
    };

    // A release variant token (`rel`/`relm`/`relsl`/`rel_2`) is its own layer
    // and takes precedence over any dynamic-suffix layer.
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
    // The note value: prefer an explicit `_<note>-<vel>` group when the name uses
    // that delimiter (e.g. "EP 1 r2_100-107", "MKS-20 Piano 1_100-111"). The
    // leading-numeric heuristic otherwise mistakes a name-number ("E Piano 1",
    // "MKS-20") for the note and collapses the whole keyboard onto it. `note_idx`
    // still drives the articulation/dynamic split, so those ids are unchanged.
    let note = underscore_note(stem).map_or_else(|| tokens[note_idx].parse::<u8>().ok(), Some)?;
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

    // Sample-layer (`SL01`, `SL02`, …) = distinct simultaneous mic/render layers
    // (direct / stereo / room). Map them onto the mic dimension so they don't all
    // collapse onto one key and overwrite each other — which made the surviving
    // layer vary note-to-note, audible as direct and room mics "colliding". SL01
    // is the default `Main` mic (what the styx declares); higher layers get their
    // own mic id, present in the map but reachable only if a spec declares them.
    // (Interim: the .db-authored zone map replaces this heuristic wholesale.)
    let mic = tokens
        .iter()
        .find_map(|t| {
            let n: u32 = t.strip_prefix("sl")?.parse().ok()?;
            (n > 1).then(|| format!("SL{n:02}"))
        })
        .unwrap_or_else(|| "Main".to_string());

    Some(SampleKey {
        section: "main".to_string(),
        articulation,
        mic,
        dynamic,
        note,
        direction,
        rr,
    })
}

/// The note from an explicit `_<note>-<vel>` group (underscore-delimited note-vel
/// schemes), taking the note of the LAST such group. `None` when the name has no
/// such group (space-separated schemes fall back to the token heuristic).
fn underscore_note(stem: &str) -> Option<u8> {
    let b = stem.as_bytes();
    let mut found = None;
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'_' {
            let mut j = i + 1;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            // digits, then '-', then at least one digit → a `<note>-<vel>` group
            if j > i + 1
                && j < b.len()
                && b[j] == b'-'
                && b.get(j + 1).is_some_and(u8::is_ascii_digit)
            {
                if let Ok(n) = stem[i + 1..j].parse::<u16>() {
                    if n <= 127 {
                        found = Some(n as u8); // keep the last match
                    }
                }
            }
        }
        i += 1;
    }
    found
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
        // The `_2` body layer is kept as a distinct blend layer, not dropped
        // (dropping it collided with the base `126` and silently lost a layer).
        assert_eq!(key.direction, "2");
        // Base body has no layer suffix.
        let base = parse_sample_stem("RR04 lacrm 60 126").unwrap();
        assert_eq!(base.direction, "");
        assert_eq!(base.dynamic, "126");
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

    /// Real filenames from the Keyscape patches whose packs shipped
    /// incomplete. The packs were missing the *files* (the out-of-tree builder
    /// that selected them is gone), not failing to parse — but a rebuild is
    /// only worth doing if the map can then reach what it packs, so these pin
    /// that down before the rebuild rather than after.
    #[test]
    fn parse_keyscape_space_separated_body_samples() {
        // Vintage Vibe EP. Its pack held 4516 release samples and zero bodies;
        // the body names carry a space *inside* the articulation token
        // ("VVEP r06") and a " v10" velocity field rather than "-100".
        let body = parse_sample_stem("RR01_SL01 VVEP r06_100 v10").expect("body parses");
        assert_eq!(body.articulation, "vvepr06", "matches the styx artic id");
        assert_eq!(body.note, 100);
        assert_eq!(body.rr, 0);
        // v01..v19 map across 1..=127, so layers cannot collapse onto one key.
        assert_eq!(body.dynamic, "67", "v10 of 19 lands mid-scale");
        let louder = parse_sample_stem("RR01_SL01 VVEP r06_100 v19").unwrap();
        assert_ne!(louder.dynamic, body.dynamic, "velocity layers stay distinct");

        // The releases from the same patch — these are what did get packed.
        let rel = parse_sample_stem("RR01_SL01 VVRFstr04_100-100").expect("release parses");
        assert_eq!(rel.articulation, "vvrfstr04");
        assert_eq!(rel.note, 100);

        // MKS-20 E Piano 1: 1320 files became 15 pack entries. Spaces run
        // right through the articulation token here.
        let mks = parse_sample_stem("MKS20 EP 1 r2_100-107").expect("MKS-20 body parses");
        assert_eq!(mks.note, 100, "the note is the _<note>-<vel> group");
        assert_eq!(mks.dynamic, "107");
        assert!(
            mks.articulation.starts_with("mks20ep"),
            "articulation keeps the name tokens, got {:?}",
            mks.articulation
        );

        // Rhodes Bass: space-separated throughout, half the files missing.
        let rbd = parse_sample_stem("RR01_SL01 rbd rel 109 28").expect("Rhodes Bass parses");
        assert_eq!(rbd.note, 109);
        assert_eq!(rbd.direction, "rel", "a release layer, not a body");
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

    /// Regression: when a note-on's round-robin index lands on an RR that the
    /// requested dynamic doesn't have, resolution must keep the requested
    /// dynamic (falling back across RR) rather than switch to a neighbouring
    /// dynamic. Keyscape LA Custom ships the long high-velocity sustains at
    /// rr0 only but 4 RRs for its mid layers — the old exact-RR filter made
    /// 3 of every 4 note-ons play a short mid-velocity sample, so held notes
    /// died right after the attack.
    #[test]
    fn resolve_prefers_requested_dynamic_over_round_robin() {
        let styx = "name \"r\"\n\
             sections ({\n\
               id main\n\
               label m\n\
               note_grid ()\n\
               lowest_note C-1\n\
               highest_note C8\n\
             })\n\
             mics ({\n\
               id Main\n\
               label Main\n\
               kind blended\n\
             })\n\
             articulations (\n\
             {\n\
               id lacrm\n\
               label \"lacrm\"\n\
               kind @OneShot\n\
               dynamics (\n\
                 \"84\"\n\
                 \"102\"\n\
               )\n\
               rr 4\n\
               dyn_ctrl velocity\n\
             })\n";
        let spec = crate::LibrarySpec::from_styx(styx).expect("parse styx");
        // Long sustain layer (dyn 102) exists at rr0 only; short mid layer
        // (dyn 84) exists at all four RRs.
        let paths: Vec<std::path::PathBuf> = vec![
            "RR01 lacrm 60 102.flac".into(),
            "RR01 lacrm 60 84.flac".into(),
            "RR02 lacrm 60 84.flac".into(),
            "RR03 lacrm 60 84.flac".into(),
            "RR04 lacrm 60 84.flac".into(),
        ];
        let map = SampleMap::from_paths(paths);

        // Every RR index requesting dyn 102 must land on the dyn-102 sample —
        // never the shorter dyn-84 neighbour — even when that exact RR is
        // absent for dyn 102.
        for rr in 0..4 {
            let (path, _) = map
                .resolve(
                    &spec,
                    &SampleQuery {
                        section_id: "main",
                        articulation_id: "lacrm",
                        mic_id: "Main",
                        dynamic: "102",
                        target_note: 60,
                        direction: "",
                        rr,
                    },
                )
                .unwrap_or_else(|| panic!("resolve failed at rr {rr}"));
            let name = path.file_name().unwrap().to_string_lossy();
            assert!(
                name.contains(" 102"),
                "rr {rr} resolved to {name}, expected the dyn-102 sustain sample"
            );
        }

        // A genuinely present RR of the mid layer still round-robins exactly.
        let (path, _) = map
            .resolve(
                &spec,
                &SampleQuery {
                    section_id: "main",
                    articulation_id: "lacrm",
                    mic_id: "Main",
                    dynamic: "84",
                    target_note: 60,
                    direction: "",
                    rr: 2,
                },
            )
            .expect("resolve dyn 84 rr2");
        assert_eq!(
            path.file_name().unwrap().to_string_lossy(),
            "RR03 lacrm 60 84.flac"
        );
    }
}

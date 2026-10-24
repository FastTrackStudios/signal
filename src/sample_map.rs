//! Sample file discovery and lookup.
//!
//! The sample map builds an index over all sample files in a library root
//! directory, then provides O(1) lookup by (section, mic, articulation,
//! dynamic, note, `rr_index`, direction).
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
    /// Nearest-dynamic index: every sample grouped by
    /// (section, articulation, mic, note), keyed by a hash of that tuple.
    ///
    /// This exists because [`SampleMap::nearest_dynamic_path`] runs on the
    /// AUDIO THREAD, once per note-on per layer per alias, and it used to
    /// answer by scanning the whole map — four `String` comparisons and a
    /// `str::parse` per entry, across every sample in the library. On a
    /// Keyscape-sized map that is tens of millions of string compares per
    /// note: measured at 50-110 ms of callback time PER NOTE, scaling
    /// linearly with the notes in a chord, against a 5.33 ms block budget.
    /// A four-note chord therefore stalled the audio thread for ~400 ms,
    /// which is what "the keys rig glitches" meant.
    ///
    /// Grouping collapses that scan to a hash lookup plus a walk of the few
    /// candidates that share a note. The dynamic is parsed once here, at
    /// build time, rather than per entry per note-on.
    ///
    /// Keyed by hash rather than by the owned tuple so the lookup allocates
    /// nothing: the group carries its own identity and is verified on hit,
    /// so a collision is a miss, never a wrong sample.
    by_note: HashMap<u64, Vec<NoteGroup>>,
    /// Total files indexed.
    total: usize,
}

/// Every sample sharing a (section, articulation, mic, note).
#[derive(Clone)]
struct NoteGroup {
    section: String,
    articulation: String,
    mic: String,
    note: u8,
    candidates: Vec<Candidate>,
}

/// One sample within a [`NoteGroup`], with its dynamic already parsed.
#[derive(Clone)]
struct Candidate {
    direction: String,
    dynamic: i16,
    rr: usize,
    path: PathBuf,
}

/// Hash of a group's identity. Cheap, and computed from borrowed strs so the
/// audio-thread lookup never builds an owned key.
fn group_hash(section: &str, articulation: &str, mic: &str, note: u8) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    section.hash(&mut h);
    articulation.hash(&mut h);
    mic.hash(&mut h);
    note.hash(&mut h);
    h.finish()
}

impl SampleMap {
    /// Build an empty map. Useful in tests.
    #[must_use]
    pub fn empty() -> Self {
        Self::from_map(HashMap::new())
    }

    /// Build both indexes from the primary map. Every constructor goes
    /// through here so the nearest-dynamic index can never drift out of step
    /// with the map it answers for.
    fn from_map(map: HashMap<SampleKey, PathBuf>) -> Self {
        let total = map.len();
        let mut by_note: HashMap<u64, Vec<NoteGroup>> = HashMap::new();
        for (key, path) in &map {
            // A dynamic that is not a number has no distance to anything, so
            // it can never be the NEAREST one; leaving it out keeps the
            // audio-thread walk to candidates that can actually be chosen.
            let Ok(dynamic) = key.dynamic.parse::<i16>() else {
                continue;
            };
            let h = group_hash(&key.section, &key.articulation, &key.mic, key.note);
            let groups = by_note.entry(h).or_default();
            let idx = if let Some(i) = groups.iter().position(|g| {
                g.section == key.section
                    && g.articulation == key.articulation
                    && g.mic == key.mic
                    && g.note == key.note
            }) {
                i
            } else {
                groups.push(NoteGroup {
                    section: key.section.clone(),
                    articulation: key.articulation.clone(),
                    mic: key.mic.clone(),
                    note: key.note,
                    candidates: Vec::new(),
                });
                groups.len() - 1
            };
            groups[idx].candidates.push(Candidate {
                direction: key.direction.clone(),
                dynamic,
                rr: key.rr,
                path: path.clone(),
            });
        }
        Self {
            map,
            by_note,
            total,
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
        Self::from_map(map)
    }

    /// Scan `root_dir` and build a sample map.
    ///
    /// Expects sample files either directly in `root_dir` (flat layout) or
    /// nested in subdirectories (organised layout). `.wav` and `.flac` files
    /// are parsed using [`parse_sample_stem`].
    ///
    /// # Errors
    ///
    /// Returns [`SamplerError`] if `root_dir` cannot be read, or if a sample
    /// file under it cannot be opened or parsed.
    pub fn scan(root_dir: &Path) -> Result<Self, SamplerError> {
        let mut map = HashMap::new();
        scan_dir(root_dir, &mut map)?;
        Ok(Self::from_map(map))
    }

    /// Total number of sample files indexed.
    #[must_use]
    pub fn total(&self) -> usize {
        self.total
    }

    /// Paths for all indexed samples.
    pub fn paths(&self) -> impl Iterator<Item = &PathBuf> {
        self.map.values()
    }

    /// Look up the exact path for a sample key.
    #[must_use]
    pub fn get(&self, key: &SampleKey) -> Option<&PathBuf> {
        self.map.get(key)
    }

    /// Resolve a playback lookup to a sample path, performing pitch-rounding
    /// to the nearest sampled note in the spec's `note_grid`.
    ///
    /// If `target_note` is not directly sampled, the nearest grid note is
    /// used and the engine is expected to transpose the sample at playback.
    #[must_use]
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
            .map_or(&[][..], |a| a.aliases.as_slice());

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
        // `lacr` releases are direction-agnostic: they are damper noise, not a
        // legato transition, so an empty direction must match any recorded one
        // rather than only the empty string.
        let direction =
            if key.direction.is_empty() && key.articulation.to_ascii_lowercase().contains("lacr") {
                None
            } else {
                Some(key.direction.as_str())
            };
        // Hash lookup, then a walk of the handful of samples that share this
        // note — NOT a scan of the library. See `by_note`.
        let h = group_hash(&key.section, &key.articulation, &key.mic, key.note);
        let group = self.by_note.get(&h)?.iter().find(|g| {
            g.section == key.section
                && g.articulation == key.articulation
                && g.mic == key.mic
                && g.note == key.note
        })?;
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
        group
            .candidates
            .iter()
            .filter(|c| direction.is_none_or(|d| c.direction == d))
            .min_by_key(|c| {
                (
                    (c.dynamic - wanted).abs(),
                    (c.rr as i32 - key.rr as i32).unsigned_abs(),
                )
            })
            .map(|c| &c.path)
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
    #[must_use]
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

    /// All (`section_id`, `articulation_id`) pairs present in the map.
    #[must_use]
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
///   (Leg / `NVLeg` / Port have direction **before** the note in CSS filenames)
///
/// Returns `None` if the stem cannot be parsed (non-CS file, etc.).
#[must_use]
pub fn parse_wav_stem(stem: &str) -> Option<SampleKey> {
    parse_sample_stem(stem)
}

#[must_use]
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
            .map_or_else(|| "127".to_string(), |(_, value)| value.to_string())
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

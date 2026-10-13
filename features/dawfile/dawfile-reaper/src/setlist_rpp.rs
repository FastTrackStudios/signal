//! Setlist RPP generation — concatenate multiple song projects into a single
//! REAPER project with all songs on a shared timeline.
//!
//! Takes `Vec<ReaperProject>` + song info (names, offsets), produces a merged
//! `ReaperProject` with shared guide tracks, per-song folders, concatenated
//! tempo, and offset markers/regions.

use std::path::{Path, PathBuf};

use crate::types::item::Item;
use crate::types::marker_region::{MarkerRegion, MarkerRegionCollection};
use crate::types::project::ReaperProject;
use crate::types::time_tempo::{TempoTimeEnvelope, TempoTimePoint};
use crate::types::track::{FolderSettings, FolderState, Track};
use crate::types::track_ops::wrap_in_folder;

/// Information about a song for concatenation.
pub struct SongInfo {
    /// Song name — used as the folder name in TRACKS/.
    pub name: String,
    /// Where this song starts in the combined timeline.
    pub global_start_seconds: f64,
    /// Duration of the song in seconds.
    pub duration_seconds: f64,
    /// Local start position within the original project (bounds.start).
    /// Items/markers in the original at `local_start` map to `global_start_seconds`.
    pub local_start: f64,
    /// Directory of the original RPP file (for resolving relative media paths).
    pub source_dir: Option<PathBuf>,
}

/// Build `SongInfo` entries from song names and durations, with a configurable
/// gap (in seconds) inserted between each song.
///
/// The gap provides breathing room between songs in the combined timeline
/// for clean transitions and visual separation.
pub fn build_song_infos(songs: &[(&str, f64)], gap_seconds: f64) -> Vec<SongInfo> {
    let mut result = Vec::with_capacity(songs.len());
    let mut offset = 0.0;

    for (i, (name, duration)) in songs.iter().enumerate() {
        result.push(SongInfo {
            name: name.to_string(),
            global_start_seconds: offset,
            duration_seconds: *duration,
            local_start: 0.0,
            source_dir: None,
        });
        offset += duration;
        if i < songs.len() - 1 {
            offset += gap_seconds; // gap between songs
        }
    }

    result
}

/// Compute the gap duration in seconds for N measures at a given tempo/time sig.
///
/// For example, 2 measures at 120 BPM in 4/4 = 2 * (4 beats * 60/120) = 4 seconds.
pub fn measures_to_seconds(measures: u32, bpm: f64, beats_per_measure: u32) -> f64 {
    let beat_duration = 60.0 / bpm;
    measures as f64 * beats_per_measure as f64 * beat_duration
}

// ── RPP Combiner ────────────────────────────────────────────────────────────

/// Options for combining RPP files.
#[derive(Default)]
pub struct CombineOptions {
    /// Gap between songs, specified as a number of measures.
    /// The gap duration is computed from the **next** song's tempo and time signature.
    /// For example, 2 measures at 120 BPM in 4/4 = 4 seconds.
    /// Default: 0 (no gap).
    pub gap_measures: u32,
    /// When true, trim each song to its marker-defined bounds
    /// (PREROLL → POSTROLL / =START → =END / SONGSTART → SONGEND).
    /// Content outside the bounds is excluded. When false, include
    /// everything from position 0 to the last content extent.
    /// Default: false (include everything).
    pub trim_to_bounds: bool,
}

/// Compute the content extent of a project (the latest point of any content).
// r[impl bounds.content-extent]
///
/// Returns the maximum of:
/// - item ends (position + length) across all tracks
/// - marker/region endpoints
/// - tempo envelope points
///
/// This ensures the combined project allocates enough space for each song's
/// full extent, including trailing tempo points that extend past audio.
fn project_content_extent(project: &ReaperProject) -> f64 {
    let max_item_end = project
        .tracks
        .iter()
        .flat_map(|t| t.items.iter())
        .map(|item| item.position + item.length)
        .fold(0.0f64, f64::max);

    let max_marker = project
        .markers_regions
        .all
        .iter()
        .map(|m| m.end_position.unwrap_or(m.position).max(m.position))
        .fold(0.0f64, f64::max);

    let max_tempo = project
        .tempo_envelope
        .as_ref()
        .map(|te| {
            te.points
                .iter()
                .map(|pt| pt.position)
                .fold(0.0f64, f64::max)
        })
        .unwrap_or(0.0);

    max_item_end.max(max_marker).max(max_tempo)
}

/// Extract the ending tempo (BPM) and beats-per-measure from a project.
///
/// Uses the last tempo envelope point (which represents the tempo at the end
/// of the song). Falls back to the TEMPO header if no envelope exists.
fn extract_ending_tempo(project: &ReaperProject) -> (f64, u32) {
    if let Some(ref te) = project.tempo_envelope
        && let Some(pt) = te.points.last()
    {
        let beats = pt
            .time_signature_encoded
            .map(|ts| (ts & 0xFFFF) as u32)
            .unwrap_or_else(|| {
                // No time sig on last point — walk backwards to find most recent
                te.points
                    .iter()
                    .rev()
                    .find_map(|p| p.time_signature_encoded.map(|ts| (ts & 0xFFFF) as u32))
                    .unwrap_or(4)
            });
        return (pt.tempo, beats.max(1));
    }
    if let Some((bpm, num, _denom, _flags)) = project.properties.tempo {
        return (bpm, (num as u32).max(1));
    }
    (120.0, 4)
}

/// Extract the starting tempo (BPM) and beats-per-measure from a project.
///
/// Uses the first tempo envelope point if available, otherwise falls back
/// to the TEMPO project property.
fn extract_project_tempo(project: &ReaperProject) -> (f64, u32) {
    // Try tempo envelope first point
    if let Some(ref te) = project.tempo_envelope
        && let Some(pt) = te.points.first()
    {
        let beats = pt
            .time_signature_encoded
            .map(|ts| (ts & 0xFFFF) as u32) // numerator in low bits
            .unwrap_or(4);
        return (pt.tempo, beats.max(1));
    }
    // Fall back to TEMPO property: (bpm, numerator, denominator, flags)
    if let Some((bpm, num, _denom, _flags)) = project.properties.tempo {
        return (bpm, (num as u32).max(1));
    }
    (120.0, 4) // default
}

/// Combine multiple RPP files into a single RPP project.
///
/// Reads each RPP, determines the full content extent of each project,
/// and lays them out sequentially on a shared timeline. Uses the raw
/// concatenation pipeline to preserve everything (FX chains, MIDI data,
/// plugin state, envelopes, fades, takes). Tempo envelopes from all
/// projects are concatenated with proper offsets.
///
/// Returns `(combined_rpp_text, song_infos)`.
// r[impl combine.rpl-to-rpp]
// r[impl combine.sequential-layout]
// r[impl combine.gap-measures]
pub fn combine_rpp_files(
    rpp_paths: &[PathBuf],
    options: &CombineOptions,
) -> crate::RppResult<(String, Vec<SongInfo>)> {
    if rpp_paths.is_empty() {
        return Err(crate::RppParseError::ParseError(
            "No RPP files to combine".to_string(),
        ));
    }

    // Parse each RPP to determine content extent
    let mut projects = Vec::with_capacity(rpp_paths.len());
    let mut names = Vec::with_capacity(rpp_paths.len());

    for path in rpp_paths {
        let content = std::fs::read_to_string(path)?;
        let project = crate::parse_project_text(&content)?;
        projects.push(project);
        names.push(song_name_from_path(path));
    }

    // Build song infos — either trimmed to marker bounds or full extent
    let mut song_infos = Vec::with_capacity(projects.len());
    let mut offset = 0.0;

    for (i, (project, name)) in projects.iter().zip(names.iter()).enumerate() {
        let (duration, local_start) = if options.trim_to_bounds {
            // Trim to marker-defined bounds (PREROLL → POSTROLL / =START → =END / etc.)
            let bounds = resolve_song_bounds(project);
            (bounds.end - bounds.start, bounds.start)
        } else {
            // Include everything from position 0 to last content
            (project_content_extent(project), 0.0)
        };

        song_infos.push(SongInfo {
            name: name.clone(),
            global_start_seconds: offset,
            duration_seconds: duration,
            local_start,
            source_dir: None,
        });

        offset += duration;

        // Add gap between songs.
        //
        // The gap is exactly `gap_measures` measures at the ending song's tempo.
        // This ensures a clean number of measures between songs regardless of
        // the accumulated tempo history.
        if options.gap_measures > 0 && i < projects.len() - 1 {
            let (bpm, beats_per_measure) = extract_ending_tempo(project);
            let gap = measures_to_seconds(options.gap_measures, bpm, beats_per_measure);
            offset += gap;
        }
    }

    // Set source_dir on each song so media paths get resolved to absolute
    for (info, path) in song_infos.iter_mut().zip(rpp_paths.iter()) {
        info.source_dir = path.parent().map(|p| p.to_path_buf());
    }

    // Use the parsed pipeline for proper track organization (folder structure,
    // guide track merging, item trimming). Then serialize to RPP text.
    let mut combined = concatenate_projects(&projects, &song_infos);
    organize_ruler_lanes(&mut combined);
    let combined_text = project_to_rpp_text(&combined);
    Ok((combined_text, song_infos))
}

/// Combine RPP files listed in an `.RPL` file into a single RPP project.
///
/// Convenience wrapper around [`combine_rpp_files`] that first parses the RPL.
/// Returns `(combined_rpp_text, song_infos)`.
pub fn combine_rpl(
    rpl_path: &Path,
    options: &CombineOptions,
) -> crate::RppResult<(String, Vec<SongInfo>)> {
    let rpp_paths = parse_rpl(rpl_path)?;
    combine_rpp_files(&rpp_paths, options)
}

/// Combine RPP files listed in an `.RPL` and write the result to disk.
///
/// Returns the `Vec<SongInfo>` with resolved positions for each song.
pub fn combine_rpl_to_file(
    rpl_path: &Path,
    output_path: &Path,
    options: &CombineOptions,
) -> crate::RppResult<Vec<SongInfo>> {
    let (combined, song_infos) = combine_rpl(rpl_path, options)?;
    std::fs::write(output_path, &combined)?;
    Ok(song_infos)
}

// ── Track Concatenation (US-004) ─────────────────────────────────────────────

// ── RPL File Parsing ─────────────────────────────────────────────────────────

/// Parse an `.RPL` file (REAPER Project List).
///
/// Each non-empty line is a path to an RPP file. Relative paths are resolved
/// against the RPL file's parent directory.
// r[impl rpl.parse]
pub fn parse_rpl(rpl_path: &Path) -> std::io::Result<Vec<PathBuf>> {
    let content = std::fs::read_to_string(rpl_path)?;
    let parent = rpl_path.parent().unwrap_or(Path::new("."));
    Ok(content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let path = PathBuf::from(line.trim());
            if path.is_absolute() {
                path
            } else {
                parent.join(path)
            }
        })
        .collect())
}

/// Extract a song name from an RPP file path.
///
/// Strips the extension and any trailing bracketed content
/// (e.g., `"Belief - John Mayer [Battle SP26].RPP"` → `"Belief - John Mayer"`).
// r[impl rpl.song-name]
pub fn song_name_from_path(path: &Path) -> String {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let name = stem.split('[').next().unwrap_or(&stem).trim();
    name.to_string()
}

// ── Track Concatenation (US-004) ─────────────────────────────────────────────

// ── Song Bounds Resolution ───────────────────────────────────────────────────

/// Resolved bounds for a song — the full range to include in the combined setlist.
#[derive(Debug, Clone)]
pub struct SongBounds {
    /// Start of the song's allocated range (seconds, relative to project start).
    pub start: f64,
    /// End of the song's allocated range.
    pub end: f64,
    /// Position of the last marker/region endpoint in the song.
    pub last_marker_position: f64,
}

/// Resolve the full bounds of a song from its markers.
///
/// Priority (outermost wins):
/// 1. PREROLL → POSTROLL
/// 2. COUNT-IN → POSTROLL (or =END if no POSTROLL)
/// 3. =START → =END
/// 4. SONGSTART → SONGEND
/// 5. First section region → last section region end
/// 6. 0 → last marker position
// r[impl bounds.resolve]
pub fn resolve_song_bounds(project: &ReaperProject) -> SongBounds {
    let markers = &project.markers_regions.all;

    let mut preroll: Option<f64> = None;
    let mut postroll: Option<f64> = None;
    let mut count_in: Option<f64> = None;
    let mut eq_start: Option<f64> = None;
    let mut eq_end: Option<f64> = None;
    let mut songstart: Option<f64> = None;
    let mut songend: Option<f64> = None;
    let mut last_pos: f64 = 0.0;
    let mut first_section_start: Option<f64> = None;
    let mut last_section_end: Option<f64> = None;

    for m in markers {
        let name_upper = m.name.to_uppercase();

        let end_pos = m.end_position.unwrap_or(m.position);
        if end_pos > last_pos {
            last_pos = end_pos;
        }
        if m.position > last_pos {
            last_pos = m.position;
        }

        match name_upper.as_str() {
            "PREROLL" | "=PREROLL" => preroll = Some(m.position),
            "POSTROLL" | "=POSTROLL" => postroll = Some(m.position),
            "COUNT-IN" | "COUNT IN" | "COUNTIN" => count_in = Some(m.position),
            "=START" => eq_start = Some(m.position),
            "=END" => eq_end = Some(m.position),
            "SONGSTART" => songstart = Some(m.position),
            "SONGEND" => songend = Some(m.position),
            _ => {
                if m.is_region() && !m.name.is_empty() {
                    if first_section_start.is_none() || m.position < first_section_start.unwrap() {
                        first_section_start = Some(m.position);
                    }
                    if let Some(end) = m.end_position
                        && (last_section_end.is_none() || end > last_section_end.unwrap())
                    {
                        last_section_end = Some(end);
                    }
                }
            }
        }
    }

    // Pick the earliest available start bound.
    // All of these markers indicate "the song starts here" — take the earliest
    // so we don't accidentally skip content before a later marker.
    let raw_start = [preroll, count_in, eq_start, songstart, first_section_start]
        .iter()
        .filter_map(|opt| *opt)
        .fold(f64::MAX, f64::min);
    let raw_start = if raw_start == f64::MAX {
        0.0
    } else {
        raw_start
    };

    // Pick the latest available end bound.
    // Prefer explicit end markers (=END, POSTROLL) over section-based detection.
    let raw_end = postroll
        .or(eq_end)
        .or(songend)
        .or(last_section_end)
        .unwrap_or(last_pos);

    // Use exact marker positions. Measure alignment is handled by the
    // combine pipeline's gap calculation, not here.
    let start = raw_start;
    let end = raw_end.max(start + 0.1);

    SongBounds {
        start,
        end,
        last_marker_position: last_pos,
    }
}

/// Snap a time position to the start of the previous (or current) measure.
///
/// If the position already falls on a measure boundary, returns it unchanged.
/// Otherwise rounds down to the previous barline.
fn snap_to_prev_measure(position: f64, project: &ReaperProject) -> f64 {
    use crate::types::time_pos_utils::time_to_beat_position_structured;

    if position <= 0.0 {
        return 0.0;
    }

    let (default_tempo, default_ts) = if let Some((bpm, num, denom, _)) = project.properties.tempo {
        (bpm, (num, denom))
    } else {
        (120.0, (4i32, 4i32))
    };

    let tempo_points = project
        .tempo_envelope
        .as_ref()
        .map(|te| te.points.as_slice())
        .unwrap_or(&[]);

    let (measure, beat, subbeat) =
        time_to_beat_position_structured(position, tempo_points, default_tempo, default_ts);

    // If already on a measure boundary, return as-is
    if beat == 1 && subbeat == 0 {
        return position;
    }

    // Target: start of current measure (measure, beat 1, subbeat 0)
    let target_measure = measure;

    // Walk tempo envelope to compute time at start of `target_measure`
    let mut current_time = 0.0;
    let mut current_tempo = default_tempo;
    let mut current_ts = default_ts;
    let mut current_measure_f = 0.0;

    let target_measures_0based = (target_measure - 1) as f64;

    for point in tempo_points {
        let segment_duration = point.position - current_time;
        if segment_duration > 0.0 {
            let tempo_ratio = current_ts.1 as f64 / 4.0;
            let effective_tempo = current_tempo * tempo_ratio;
            let segment_beats = segment_duration * effective_tempo / 60.0;
            let beats_per_measure = current_ts.0 as f64;
            let segment_measures = segment_beats / beats_per_measure;

            if current_measure_f + segment_measures >= target_measures_0based {
                let remaining_measures = target_measures_0based - current_measure_f;
                let remaining_beats = remaining_measures * beats_per_measure;
                let remaining_time = remaining_beats * 60.0 / effective_tempo;
                return current_time + remaining_time;
            }

            current_measure_f += segment_measures;
        }

        current_time = point.position;
        current_tempo = point.tempo;
        if let Some(ts) = point.time_signature() {
            current_ts = ts;
        }
    }

    let remaining_measures = target_measures_0based - current_measure_f;
    if remaining_measures <= 0.0 {
        return current_time;
    }
    let tempo_ratio = current_ts.1 as f64 / 4.0;
    let effective_tempo = current_tempo * tempo_ratio;
    let beats_per_measure = current_ts.0 as f64;
    let remaining_beats = remaining_measures * beats_per_measure;
    let remaining_time = remaining_beats * 60.0 / effective_tempo;
    current_time + remaining_time
}

/// Snap a time position to the start of the next measure.
///
/// If the position already falls exactly on a measure boundary (within 1ms),
/// returns that position unchanged. Otherwise rounds up to the next barline.
///
/// Uses `time_to_beat_position_structured` to accurately determine the current
/// musical position (accounting for all tempo and time signature changes), then
/// walks the tempo envelope forward to compute the time of the next measure start.
fn snap_to_next_measure(position: f64, project: &ReaperProject) -> f64 {
    use crate::types::time_pos_utils::time_to_beat_position_structured;

    // Get default tempo and time signature from project
    let (default_tempo, default_ts) = if let Some((bpm, num, denom, _)) = project.properties.tempo {
        (bpm, (num, denom))
    } else {
        (120.0, (4i32, 4i32))
    };

    let tempo_points = project
        .tempo_envelope
        .as_ref()
        .map(|te| te.points.as_slice())
        .unwrap_or(&[]);

    // Get the musical position at the given time
    let (measure, beat, subbeat) =
        time_to_beat_position_structured(position, tempo_points, default_tempo, default_ts);

    // If we're already at beat 1, subbeat 0 — we're on a measure boundary
    if beat == 1 && subbeat == 0 {
        return position;
    }

    // Target: start of the next measure (measure + 1, beat 1, subbeat 0)
    let target_measure = measure + 1;

    // Walk the tempo envelope to compute time at the start of `target_measure`.
    // This is the inverse of time_to_beat_position_structured.
    let mut current_time = 0.0;
    let mut current_tempo = default_tempo;
    let mut current_ts = default_ts;
    let mut current_measure_f = 0.0; // 0-based fractional measure count

    let target_measures_0based = (target_measure - 1) as f64; // convert 1-based to 0-based

    for point in tempo_points {
        // Calculate measures from current_time to this point
        let segment_duration = point.position - current_time;
        if segment_duration > 0.0 {
            let tempo_ratio = current_ts.1 as f64 / 4.0;
            let effective_tempo = current_tempo * tempo_ratio;
            let segment_beats = segment_duration * effective_tempo / 60.0;
            let beats_per_measure = current_ts.0 as f64;
            let segment_measures = segment_beats / beats_per_measure;

            // Would passing this point overshoot our target?
            if current_measure_f + segment_measures >= target_measures_0based {
                // Target is within this segment
                let remaining_measures = target_measures_0based - current_measure_f;
                let remaining_beats = remaining_measures * beats_per_measure;
                let remaining_time = remaining_beats * 60.0 / effective_tempo;
                return current_time + remaining_time;
            }

            current_measure_f += segment_measures;
        }

        current_time = point.position;
        current_tempo = point.tempo;
        if let Some(ts) = point.time_signature() {
            current_ts = ts;
        }
    }

    // Target is past all tempo points — use final tempo
    let remaining_measures = target_measures_0based - current_measure_f;
    if remaining_measures <= 0.0 {
        return current_time;
    }
    let tempo_ratio = current_ts.1 as f64 / 4.0;
    let effective_tempo = current_tempo * tempo_ratio;
    let beats_per_measure = current_ts.0 as f64;
    let remaining_beats = remaining_measures * beats_per_measure;
    let remaining_time = remaining_beats * 60.0 / effective_tempo;
    current_time + remaining_time
}

/// Build `SongInfo` entries from parsed projects using resolved bounds + gap.
pub fn build_song_infos_from_projects(
    projects: &[ReaperProject],
    names: &[&str],
    gap_seconds: f64,
) -> Vec<SongInfo> {
    assert_eq!(projects.len(), names.len());
    let mut result = Vec::with_capacity(projects.len());
    let mut offset = 0.0;

    for (i, (project, name)) in projects.iter().zip(names.iter()).enumerate() {
        let bounds = resolve_song_bounds(project);
        let duration = bounds.end - bounds.start;

        result.push(SongInfo {
            name: name.to_string(),
            global_start_seconds: offset,
            duration_seconds: duration,
            local_start: bounds.start,
            source_dir: None, // Set by caller after construction
        });

        offset += duration;
        if i < projects.len() - 1 {
            offset += gap_seconds;
        }
    }

    result
}

// ── Track Concatenation (US-004) ─────────────────────────────────────────────

/// Well-known guide track names that get merged across songs into the
/// Click + Guide header folder.
const GUIDE_TRACK_NAMES: &[&str] = &["Click", "Loop", "Count", "Guide"];

/// Well-known Keyflow track names that get merged across songs into the
/// Keyflow header folder.
const KEYFLOW_TRACK_NAMES: &[&str] = &["CHORDS", "LINES", "HITS"];

/// All header track names that get merged (guide + keyflow).
fn is_header_track(name: &str) -> bool {
    let lower = name.to_lowercase();
    GUIDE_TRACK_NAMES.iter().any(|g| g.to_lowercase() == lower)
        || KEYFLOW_TRACK_NAMES
            .iter()
            .any(|k| k.to_lowercase() == lower)
}

/// Concatenate tracks from multiple projects into a single track list.
///
/// Header tracks are merged across songs:
/// - **Click + Guide**: Click, Loop, Count, Guide
/// - **Keyflow**: CHORDS, LINES, HITS
///
/// Content tracks appear under `TRACKS/{Song Name}/` folder hierarchy.
pub fn concatenate_tracks(projects: &[ReaperProject], songs: &[SongInfo]) -> Vec<Track> {
    assert_eq!(projects.len(), songs.len());

    let mut header_tracks: std::collections::HashMap<String, Track> =
        std::collections::HashMap::new();
    let mut all_song_tracks: Vec<(String, Vec<Track>)> = Vec::new();
    let mut all_song_reference: Vec<(String, Vec<Track>)> = Vec::new();

    for (project, song) in projects.iter().zip(songs.iter()) {
        let offset = song.global_start_seconds - song.local_start;

        let mut song_content: Vec<Track> = Vec::new();
        let local_end = song.local_start + song.duration_seconds;

        let source_items: usize = project.tracks.iter().map(|t| t.items.len()).sum();
        let mut skipped_structural = 0usize;
        let mut items_before_trim: usize = 0;
        let mut items_after_trim: usize = 0;

        for track in &project.tracks {
            let name_lower = track.name.to_lowercase();
            let is_header = is_header_track(&track.name);
            let is_structural = is_structural_folder(&name_lower, track);

            if is_structural {
                skipped_structural += 1;
                continue;
            }

            items_before_trim += track.items.len();

            let mut cloned = clone_track_with_offset(
                track,
                offset,
                song.source_dir.as_deref(),
                song.local_start,
                Some(local_end),
            );
            cloned.track_id = None;

            items_after_trim += cloned.items.len();

            if is_header {
                cloned.folder = None;
                header_tracks
                    .entry(track.name.clone())
                    .and_modify(|existing| {
                        existing.items.extend(cloned.items.clone());
                    })
                    .or_insert(cloned);
            } else {
                cloned.folder = None;
                song_content.push(cloned);
            }
        }

        eprintln!(
            "[combine] {}: {} source items, {} structural skipped, {}/{} items before/after trim (local {:.1}..{:.1})",
            song.name,
            source_items,
            skipped_structural,
            items_before_trim,
            items_after_trim,
            song.local_start,
            local_end,
        );

        // Split song content into regular tracks vs reference tracks
        let mut song_tracks: Vec<Track> = Vec::new();
        let mut song_reference: Vec<Track> = Vec::new();

        for track in song_content {
            let lower = track.name.to_lowercase();
            if lower == "reference" || lower == "stem split" || lower == "mix" {
                song_reference.push(track);
            } else {
                song_tracks.push(track);
            }
        }

        if !song_tracks.is_empty() {
            all_song_tracks.push((song.name.clone(), song_tracks));
        }
        if !song_reference.is_empty() {
            all_song_reference.push((song.name.clone(), song_reference));
        }
    }

    let mut result: Vec<Track> = Vec::new();

    // ── Click + Guide folder ──────────────────────────────────────
    let mut guide_children: Vec<Track> = Vec::new();
    for guide_name in GUIDE_TRACK_NAMES {
        if let Some(track) = header_tracks.remove(*guide_name) {
            guide_children.push(track);
        }
    }
    if !guide_children.is_empty() {
        result.extend(wrap_in_folder("Click + Guide", guide_children));
    }

    // ── Keyflow folder ────────────────────────────────────────────
    let mut keyflow_children: Vec<Track> = Vec::new();
    for keyflow_name in KEYFLOW_TRACK_NAMES {
        if let Some(track) = header_tracks.remove(*keyflow_name) {
            keyflow_children.push(track);
        }
    }
    if !keyflow_children.is_empty() {
        result.extend(wrap_in_folder("Keyflow", keyflow_children));
    }

    // ── TRACKS folder (per-song content) ──────────────────────────
    if !all_song_tracks.is_empty() {
        let mut tracks_children: Vec<Track> = Vec::new();
        for (song_name, tracks) in all_song_tracks {
            tracks_children.extend(wrap_in_folder(&song_name, tracks));
        }
        result.extend(wrap_in_folder("TRACKS", tracks_children));
    }

    // ── Reference folder (per-song reference/stem split) ──────────
    if !all_song_reference.is_empty() {
        let mut ref_children: Vec<Track> = Vec::new();
        for (song_name, tracks) in all_song_reference {
            ref_children.extend(wrap_in_folder(&song_name, tracks));
        }
        result.extend(wrap_in_folder("Reference", ref_children));
    }

    result
}

/// Patch the SOFFS value in an item's raw_content string.
fn patch_soffs(raw_content: &str, new_soffs: f64) -> String {
    let mut result = Vec::new();
    let mut found = false;
    for line in raw_content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("SOFFS ") {
            result.push(format!("SOFFS {}", new_soffs));
            found = true;
        } else {
            result.push(line.to_string());
        }
    }
    if !found && new_soffs > 0.0 {
        // Insert SOFFS after POSITION or at the start
        let mut inserted = false;
        let mut final_result = Vec::new();
        for line in &result {
            final_result.push(line.clone());
            if !inserted && line.trim().starts_with("POSITION ") {
                final_result.push(format!("SOFFS {}", new_soffs));
                inserted = true;
            }
        }
        if !inserted {
            final_result.insert(0, format!("SOFFS {}", new_soffs));
        }
        return final_result.join("\n");
    }
    result.join("\n")
}

fn is_structural_folder(name_lower: &str, track: &Track) -> bool {
    let is_folder = track
        .folder
        .as_ref()
        .is_some_and(|f| f.folder_state == FolderState::FolderParent);
    let is_structural_name = matches!(
        name_lower,
        "click/guide" | "click + guide" | "tracks" | "keyflow" | "midi bus"
    );
    is_structural_name && is_folder
}

fn clone_track_with_offset(
    track: &Track,
    offset_seconds: f64,
    source_dir: Option<&Path>,
    local_start: f64,
    local_end: Option<f64>,
) -> Track {
    let mut cloned = track.clone();

    // Filter out items that start before the song's start bound.
    // Items that overlap the start boundary are trimmed (position moved,
    // length shortened, source offset advanced so playback starts at the
    // correct point in the source media).
    cloned.items.retain_mut(|item| {
        if item.position + item.length <= local_start {
            // Entirely before start — remove
            return false;
        }
        if item.position < local_start {
            // Partially before start — trim the beginning
            let trim = local_start - item.position;
            item.position = local_start;
            item.length -= trim;
            item.snap_offset = (item.snap_offset - trim).max(0.0);
            // Advance source offset so playback starts at the right point
            item.slip_offset += trim;
            for take in &mut item.takes {
                take.slip_offset += trim;
            }
            // Patch SOFFS in raw_content if present
            if !item.raw_content.is_empty() {
                item.raw_content = patch_soffs(&item.raw_content, item.slip_offset);
            }
        }
        true
    });

    // Filter out items that start at or after the song's end bound
    if let Some(end) = local_end {
        cloned.items.retain(|item| item.position < end);
        // Truncate items that start before the end but extend past it
        for item in &mut cloned.items {
            let item_end = item.position + item.length;
            if item_end > end {
                item.length = end - item.position;
            }
        }
    }

    for item in &mut cloned.items {
        item.position += offset_seconds;

        if let Some(dir) = source_dir {
            // Resolve relative file paths in parsed take sources
            for take in &mut item.takes {
                if let Some(ref mut source) = take.source
                    && !source.file_path.is_empty()
                    && !PathBuf::from(&source.file_path).is_absolute()
                {
                    let absolute = dir.join(&source.file_path);
                    source.file_path = absolute.to_string_lossy().to_string();
                }
            }

            // Also resolve FILE paths in raw_content so raw-content-based
            // serialization gets absolute paths too
            if !item.raw_content.is_empty() {
                let mut patched_lines = Vec::new();
                for line in item.raw_content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("FILE ") {
                        let file_path = trimmed.trim_start_matches("FILE ").trim_matches('"');
                        if !PathBuf::from(file_path).is_absolute() {
                            let absolute = dir.join(file_path);
                            patched_lines.push(format!("FILE \"{}\"", absolute.to_string_lossy()));
                            continue;
                        }
                    }
                    patched_lines.push(line.to_string());
                }
                item.raw_content = patched_lines.join("\n");
            }
        }
    }
    cloned
}

fn empty_track(name: &str) -> Track {
    Track {
        name: name.to_string(),
        ..Track::default()
    }
}

fn make_folder_track(name: &str) -> Track {
    Track {
        name: name.to_string(),
        folder: Some(FolderSettings {
            folder_state: FolderState::FolderParent,
            indentation: 0,
        }),
        ..Track::default()
    }
}

// ── Tempo Envelope Concatenation (US-005) ────────────────────────────────────

/// Concatenate tempo envelopes from multiple projects.
///
/// Each song's tempo points are offset by `global_start_seconds`. A square-shape
/// boundary point is inserted at each song transition for clean tempo changes.
pub fn concatenate_tempo_envelopes(
    projects: &[ReaperProject],
    songs: &[SongInfo],
) -> TempoTimeEnvelope {
    assert_eq!(projects.len(), songs.len());

    let mut points: Vec<TempoTimePoint> = Vec::new();

    for (project, song) in projects.iter().zip(songs.iter()) {
        let offset = song.global_start_seconds - song.local_start;

        if let Some(ref envelope) = project.tempo_envelope {
            let mut last_tempo = envelope.default_tempo;
            let local_end = song.local_start + song.duration_seconds;

            for point in &envelope.points {
                // Skip points outside the song's bounds
                if point.position < song.local_start {
                    // Track the tempo at this point so we know what
                    // tempo was active at local_start
                    last_tempo = point.tempo;
                    continue;
                }
                if point.position >= local_end {
                    continue;
                }

                let mut p = point.clone();
                p.position += offset;
                p.shape = 1; // Force square (instant) for combined setlists
                last_tempo = p.tempo;
                points.push(p);
            }

            // Ensure there's a tempo point at the song's start position.
            // If no points were within bounds (all filtered), create a leading
            // point with the tempo that was active at local_start.
            let song_global_start = song.global_start_seconds;
            let has_point_at_start = points
                .iter()
                .any(|p| (p.position - song_global_start).abs() < 0.01);
            if !has_point_at_start {
                let mut leading = TempoTimePoint {
                    position: song_global_start,
                    tempo: last_tempo,
                    shape: 1,
                    ..TempoTimePoint::default()
                };
                if let Some((_, num, denom, _)) = project.properties.tempo {
                    leading.time_signature_encoded = Some(65536 * denom + num);
                }
                // Insert at the correct position (before any points from this song)
                let insert_idx = points
                    .iter()
                    .position(|p| p.position > song_global_start)
                    .unwrap_or(points.len());
                points.insert(insert_idx, leading);
            }

            // Insert a trailing tempo marker at the song's end to freeze tempo
            let trailing_pos = song_global_start + song.duration_seconds;
            if trailing_pos > points.last().map(|p| p.position).unwrap_or(0.0) {
                let trailing = TempoTimePoint {
                    position: trailing_pos,
                    tempo: last_tempo,
                    shape: 1,
                    ..TempoTimePoint::default()
                };
                points.push(trailing);
            }
        }
    }

    let (default_tempo, default_ts) = projects
        .first()
        .and_then(|p| p.tempo_envelope.as_ref())
        .map(|e| (e.default_tempo, e.default_time_signature))
        .unwrap_or((120.0, (4, 4)));

    TempoTimeEnvelope {
        points,
        default_tempo,
        default_time_signature: default_ts,
    }
}

// ── Region/Marker Generation (US-006) ────────────────────────────────────────

/// Generate regions and markers for the setlist project.
///
/// Creates a full-song region for each song, plus offset copies of each
/// song's internal markers/regions with re-numbered IDs.
pub fn generate_markers_regions(
    projects: &[ReaperProject],
    songs: &[SongInfo],
) -> MarkerRegionCollection {
    assert_eq!(projects.len(), songs.len());

    let mut all: Vec<MarkerRegion> = Vec::new();
    let mut next_id: i32 = 1;

    for (project, song) in projects.iter().zip(songs.iter()) {
        let offset = song.global_start_seconds - song.local_start;
        let bounds = resolve_song_bounds(project);

        // SONG lane region — spans the full song bounds + a tiny bit past the last marker.
        // The extra 0.1s ensures the region visually covers the trailing tempo marker.
        let song_region_end = offset + (bounds.last_marker_position - bounds.start) + 0.1;
        all.push(MarkerRegion {
            id: next_id,
            position: offset,
            name: song.name.clone(),
            color: 0,
            flags: 0,
            locked: 0,
            guid: String::new(),
            additional: 0,
            end_position: Some(song_region_end.max(offset + song.duration_seconds)),
            lane: Some(3), // SONG lane (index 3)
            beat_position: None,
        });
        next_id += 1;

        // Offset copies of internal markers/regions with lane classification
        for mr in &project.markers_regions.all {
            let mut cloned = mr.clone();
            cloned.position += offset;
            if let Some(ref mut end) = cloned.end_position {
                *end += offset;
            }
            cloned.id = next_id;
            cloned.guid = String::new();

            // Classify into the correct ruler lane if not already set
            if cloned.lane.is_none() || cloned.lane == Some(0) {
                cloned.lane = Some(classify_lane(&cloned.name, cloned.is_region()) as i32);
            }

            all.push(cloned);
            next_id += 1;
        }
    }

    let markers: Vec<MarkerRegion> = all.iter().filter(|m| m.is_marker()).cloned().collect();
    let regions: Vec<MarkerRegion> = all.iter().filter(|m| m.is_region()).cloned().collect();

    MarkerRegionCollection {
        all,
        markers,
        regions,
    }
}

// ── Full Project Concatenation ───────────────────────────────────────────────

/// Concatenate multiple REAPER projects into a single setlist project.
pub fn concatenate_projects(projects: &[ReaperProject], songs: &[SongInfo]) -> ReaperProject {
    assert!(!projects.is_empty());
    assert_eq!(projects.len(), songs.len());

    let mut combined = projects[0].clone();
    combined.tracks = concatenate_tracks(projects, songs);
    combined.tempo_envelope = Some(concatenate_tempo_envelopes(projects, songs));
    combined.markers_regions = generate_markers_regions(projects, songs);
    combined.items.clear();
    combined
}

// ── Shell Copy Generation ────────────────────────────────────────────────────

/// Generate a shell copy of a setlist project for a specific role.
///
/// A shell copy preserves the timeline structure (tempo, markers, regions,
/// ruler lanes) and the Click/Guide tracks, but strips all content tracks.
/// A placeholder folder for the role's own tracks is added.
///
/// This enables role-specific REAPER instances (Vocals, Guitar, Keys, etc.)
/// to share the same timeline and click track while having their own
/// independent track setup.
pub fn generate_shell_copy(master: &ReaperProject, role: &str) -> ReaperProject {
    let mut shell = master.clone();

    // Strip content tracks — keep only Click/Guide tracks
    let mut kept_tracks: Vec<Track> = Vec::new();
    let mut in_guide_folder = false;
    let mut guide_depth: i32 = 0;

    for track in &master.tracks {
        let name_lower = track.name.to_lowercase();
        let is_folder_start = track
            .folder
            .as_ref()
            .is_some_and(|f| f.folder_state == FolderState::FolderParent);
        let is_folder_end = track
            .folder
            .as_ref()
            .is_some_and(|f| f.folder_state == FolderState::LastInFolder);

        // Track the Click/Guide folder hierarchy
        if name_lower == "click/guide" && is_folder_start {
            in_guide_folder = true;
            guide_depth = 1;
            kept_tracks.push(track.clone());
            continue;
        }

        if in_guide_folder {
            kept_tracks.push(track.clone());
            if is_folder_start {
                guide_depth += 1;
            }
            if is_folder_end {
                guide_depth += track.folder.as_ref().map_or(0, |f| f.indentation);
                if guide_depth <= 0 {
                    in_guide_folder = false;
                }
            }
            continue;
        }

        // Also keep individual Click/Loop/Count/Guide tracks at the top level
        // (in case they're not inside a Click/Guide folder)
        if GUIDE_TRACK_NAMES
            .iter()
            .any(|g| g.to_lowercase() == name_lower)
            && !is_folder_start
        {
            kept_tracks.push(track.clone());
            continue;
        }

        // Skip everything else (TRACKS folder, song folders, content tracks)
    }

    // Add a role folder for the performer's own tracks
    let mut role_folder = Track {
        name: role.to_string(),
        folder: Some(FolderSettings {
            folder_state: FolderState::FolderParent,
            indentation: 0,
        }),
        ..Track::default()
    };

    // Add a placeholder child track inside the role folder
    let mut placeholder = Track {
        name: format!("{} (add tracks here)", role),
        folder: Some(FolderSettings {
            folder_state: FolderState::LastInFolder,
            indentation: -1,
        }),
        ..Track::default()
    };

    kept_tracks.push(role_folder);
    kept_tracks.push(placeholder);

    shell.tracks = kept_tracks;

    // Clear items from the top-level items list (they're inside tracks)
    shell.items.clear();

    shell
}

/// Generate shell copies for multiple roles from a master setlist.
///
/// Returns a vec of (role_name, project) pairs.
pub fn generate_role_setlists(
    master: &ReaperProject,
    roles: &[&str],
) -> Vec<(String, ReaperProject)> {
    roles
        .iter()
        .map(|role| {
            let shell = generate_shell_copy(master, role);
            (role.to_string(), shell)
        })
        .collect()
}

/// Standard FTS roles for setlist shell copies.
pub const STANDARD_ROLES: &[&str] = &[
    "Vocals", "Guitar", "Guitar 2", "Keys", "Keys 2", "Bass", "Drums",
];

/// Write all role setlists to a directory.
///
/// Each file is named `{role} - {setlist_name}.RPP`.
/// Returns the paths of the written files.
pub fn write_role_setlists(
    master: &ReaperProject,
    roles: &[&str],
    setlist_name: &str,
    output_dir: &Path,
) -> std::io::Result<Vec<PathBuf>> {
    std::fs::create_dir_all(output_dir)?;

    let role_projects = generate_role_setlists(master, roles);
    let mut paths = Vec::new();

    for (role, project) in &role_projects {
        let filename = format!("{} - {}.RPP", role, setlist_name);
        let path = output_dir.join(&filename);
        let rpp_text = project_to_rpp_text(project);
        std::fs::write(&path, &rpp_text)?;
        paths.push(path);
    }

    // Also write the master as "Tracks - {name}.RPP"
    let master_filename = format!("Tracks - {}.RPP", setlist_name);
    let master_path = output_dir.join(&master_filename);
    let master_text = project_to_rpp_text(master);
    std::fs::write(&master_path, &master_text)?;
    paths.insert(0, master_path);

    Ok(paths)
}

// ── RPP Serialization ────────────────────────────────────────────────────────

/// Write a combined `ReaperProject` to RPP text.
///
/// This produces a minimal but valid RPP that REAPER can open.
/// Track items, markers, regions, and tempo envelope are included.
pub fn project_to_rpp_text(project: &ReaperProject) -> String {
    let mut out = String::new();
    let tempo = project
        .tempo_envelope
        .as_ref()
        .map(|e| e.default_tempo)
        .unwrap_or(120.0);
    let (ts_num, ts_denom) = project
        .tempo_envelope
        .as_ref()
        .map(|e| e.default_time_signature)
        .unwrap_or((4, 4));

    out.push_str("<REAPER_PROJECT 0.1 \"7.0/generated\" 0\n");
    out.push_str("  RIPPLE 0 0\n");
    out.push_str("  GROUPOVERRIDE 0 0 0 0\n");
    out.push_str("  AUTOXFADE 129\n");
    out.push_str("  ENVATTACH 3\n");
    out.push_str("  MIXERUIFLAGS 11 48\n");
    out.push_str("  PEAKGAIN 1\n");
    out.push_str("  FEEDBACK 0\n");
    out.push_str("  PANLAW 1\n");
    out.push_str("  PROJOFFS 0 0 0\n");
    out.push_str("  MAXPROJLEN 0 0\n");
    out.push_str("  GRID 3199 8 1 8 1 0 0 0\n");
    out.push_str("  TIMEMODE 1 5 -1 30 0 0 -1\n");
    out.push_str("  PANMODE 3\n");
    out.push_str("  CURSOR 0\n");
    out.push_str("  ZOOM 20 0 0\n");
    out.push_str("  VZOOMEX 6 0\n");
    out.push_str("  USE_REC_CFG 0\n");
    out.push_str("  RECMODE 1\n");
    out.push_str("  LOOP 0\n");
    out.push_str("  LOOPGRAN 0 4\n");
    out.push_str("  RECORD_PATH \"\" \"\"\n");
    out.push_str("  RENDER_FILE \"\"\n");
    out.push_str("  RENDER_PATTERN \"\"\n");
    out.push_str("  RENDER_FMT 0 2 0\n");
    out.push_str("  RENDER_1X 0\n");
    out.push_str("  RENDER_RANGE 1 0 0 18 1000\n");
    out.push_str("  SAMPLERATE 48000 0 0\n");
    out.push_str("  GLOBAL_AUTO -1\n");
    out.push_str(&format!("  TEMPO {} {} {} 0\n", tempo, ts_num, ts_denom));
    out.push_str("  PLAYRATE 1 0 0.25 4\n");
    out.push_str("  SELECTION 0 0\n");
    out.push_str("  SELECTION2 0 0\n");
    out.push_str("  MASTERTRACKHEIGHT 0 0\n");
    out.push_str("  MASTERPEAKCOL 16576\n");
    out.push_str("  MASTERMUTESOLO 0\n");
    out.push_str("  MASTERTRACKVIEW 0 0.6667 0.5 0.5 -1 -1 -1 0 0 0 -1 -1 0\n");
    out.push_str("  MASTERHWOUT 0 0 1 0 0 0 0 -1\n");
    out.push_str("  MASTER_NCH 2 2\n");
    out.push_str("  MASTER_VOLUME 1 0 -1 -1 1\n");
    out.push_str("  MASTER_PANMODE 6\n");
    out.push_str("  MASTER_FX 1\n");
    out.push_str("  MASTER_SEL 0\n");

    // Tempo envelope
    if let Some(ref env) = project.tempo_envelope {
        out.push_str("  <TEMPOENVEX\n");
        out.push_str("    ACT 1 -1\n");
        out.push_str("    VIS 1 0 1\n");
        out.push_str("    LANEHEIGHT 0 0\n");
        out.push_str("    ARM 0\n");
        out.push_str("    DEFSHAPE 1 -1 -1\n");
        for pt in &env.points {
            let ts_str = pt
                .time_signature_encoded
                .map(|ts| format!(" {}", ts))
                .unwrap_or_default();
            out.push_str(&format!(
                "    PT {:.12} {:.10} {}{}\n",
                pt.position, pt.tempo, pt.shape, ts_str
            ));
        }
        out.push_str("  >\n");
    }

    // Ruler lane definitions (FTS standard layout)
    // These must come before markers so REAPER knows the lane names
    out.push_str("  RULERHEIGHT 120 84\n");
    out.push_str("  RULERLANE 1 8 SECTIONS 0 -1\n"); // flag 8 = default region lane
    out.push_str("  RULERLANE 2 0 MARKS 0 -1\n");
    out.push_str("  RULERLANE 3 4 SONG 0 -1\n"); // flag 4 = default marker lane
    out.push_str("  RULERLANE 4 0 START/END 0 -1\n");
    out.push_str("  RULERLANE 5 0 KEY 0 -1\n");
    out.push_str("  RULERLANE 6 0 MODE 0 -1\n");
    out.push_str("  RULERLANE 7 0 CHORDS 0 -1\n");
    out.push_str("  RULERLANE 8 0 NOTES 0 -1\n");

    // Markers and regions — preserve original color, flags, GUID where available
    for mr in &project.markers_regions.all {
        let lane = mr.lane.unwrap_or(0);
        let guid_str = if mr.guid.is_empty() {
            "{}".to_string()
        } else if mr.guid.starts_with('{') {
            mr.guid.clone()
        } else {
            format!("{{{}}}", mr.guid)
        };
        if mr.is_region() {
            // Region: two MARKER lines with same ID
            out.push_str(&format!(
                "  MARKER {} {} {:?} {} {} 1 R {} {} {}\n",
                mr.id, mr.position, mr.name, mr.flags, mr.color, guid_str, mr.additional, lane
            ));
            out.push_str(&format!(
                "  MARKER {} {} \"\" {} {} 1 R {} {} {}\n",
                mr.id,
                mr.end_position.unwrap_or(mr.position),
                mr.flags,
                mr.color,
                guid_str,
                mr.additional,
                lane
            ));
        } else {
            out.push_str(&format!(
                "  MARKER {} {} {:?} {} {} 1 B {} {} {}\n",
                mr.id, mr.position, mr.name, mr.flags, mr.color, guid_str, mr.additional, lane
            ));
        }
    }

    // Tracks
    for track in &project.tracks {
        write_track_rpp(&mut out, track, 1);
    }

    out.push_str(">\n");
    out
}

fn write_track_rpp(out: &mut String, track: &Track, indent: usize) {
    let prefix = "  ".repeat(indent);

    if !track.raw_content.is_empty() {
        // Use raw content — preserves FX chains, envelopes, sends, etc.
        // But we need to patch item positions (they've been offset).
        // The raw_content includes <ITEM> blocks, so we write the track
        // header with patched ISBUS, then the raw content for everything
        // between the header and items, then our offset items.
        //
        // For simplicity, rebuild the track header but use raw items.
        write_track_header(out, track, &prefix);
        for item in &track.items {
            write_item_rpp(out, item, indent + 1, None);
        }
        out.push_str(&format!("{}>\n", prefix));
    } else {
        // No raw content — build from parsed fields
        write_track_header(out, track, &prefix);
        for item in &track.items {
            write_item_rpp(out, item, indent + 1, None);
        }
        out.push_str(&format!("{}>\n", prefix));
    }
}

fn write_track_header(out: &mut String, track: &Track, prefix: &str) {
    // Write track ID if available, otherwise empty braces
    let track_id = track.track_id.as_deref().unwrap_or("");
    if track_id.is_empty() {
        out.push_str(&format!("{}<TRACK {{}}\n", prefix));
    } else {
        out.push_str(&format!("{}<TRACK {{{}}}\n", prefix, track_id));
    }
    out.push_str(&format!("{}  NAME {:?}\n", prefix, track.name));
    out.push_str(&format!(
        "{}  PEAKCOL {}\n",
        prefix,
        track.peak_color.unwrap_or(16576)
    ));
    out.push_str(&format!("{}  BEAT -1\n", prefix));
    out.push_str(&format!("{}  AUTOMODE 0\n", prefix));

    if let Some(ref vp) = track.volpan {
        out.push_str(&format!(
            "{}  VOLPAN {} {} -1 -1 1\n",
            prefix, vp.volume, vp.pan
        ));
    } else {
        out.push_str(&format!("{}  VOLPAN 1 0 -1 -1 1\n", prefix));
    }

    out.push_str(&format!("{}  MUTESOLO 0 0 0\n", prefix));
    out.push_str(&format!("{}  IPHASE 0\n", prefix));

    if let Some(ref f) = track.folder {
        let state: i32 = match f.folder_state {
            FolderState::Regular => 0,
            FolderState::FolderParent => 1,
            FolderState::LastInFolder => 2,
            FolderState::Unknown(v) => v,
        };
        out.push_str(&format!("{}  ISBUS {} {}\n", prefix, state, f.indentation));
    } else {
        out.push_str(&format!("{}  ISBUS 0 0\n", prefix));
    }

    out.push_str(&format!("{}  SEL 0\n", prefix));
    out.push_str(&format!("{}  REC 0 0 0 0 0 0 0 0\n", prefix));
    out.push_str(&format!("{}  FX 1\n", prefix));
    out.push_str(&format!("{}  NCHAN {}\n", prefix, track.channel_count));
}

fn write_item_rpp(out: &mut String, item: &Item, indent: usize, item_source_dir: Option<&Path>) {
    let prefix = "  ".repeat(indent);

    // Use raw_content as the primary source of truth — it preserves ALL item
    // data including takes, sources, MIDI content, FX, envelopes, etc.
    // We only patch POSITION and resolve relative FILE paths to absolute.
    out.push_str(&format!("{}<ITEM\n", prefix));
    out.push_str(&format!("{}  POSITION {}\n", prefix, item.position));

    if !item.raw_content.is_empty() {
        // Write raw content, patching POSITION and FILE paths
        for line in item.raw_content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("POSITION ") {
                continue;
            }
            // Patch relative FILE paths to absolute
            if trimmed.starts_with("FILE ")
                && let Some(source_dir) = item_source_dir
            {
                // Extract the path (may be quoted)
                let file_path = trimmed.trim_start_matches("FILE ").trim_matches('"');
                if !PathBuf::from(file_path).is_absolute() {
                    let absolute = source_dir.join(file_path);
                    out.push_str(&format!(
                        "{}  FILE {:?}\n",
                        prefix,
                        absolute.to_string_lossy()
                    ));
                    continue;
                }
            }
            out.push_str(&format!("{}  {}\n", prefix, trimmed));
        }
    } else {
        // No raw content — write minimal from parsed fields
        out.push_str(&format!("{}  SNAPOFFS {}\n", prefix, item.snap_offset));
        out.push_str(&format!("{}  LENGTH {}\n", prefix, item.length));
        out.push_str(&format!("{}  LOOP 0\n", prefix));
        out.push_str(&format!("{}  ALLTAKES 0\n", prefix));
        out.push_str(&format!("{}  SEL 0\n", prefix));
        if !item.name.is_empty() {
            out.push_str(&format!("{}  NAME {:?}\n", prefix, item.name));
        }
        // Write parsed source blocks for items without raw_content
        for take in &item.takes {
            if let Some(ref source) = take.source {
                use crate::types::item::SourceType;
                let type_str = match source.source_type {
                    SourceType::Wave => "WAVE",
                    SourceType::Midi => "MIDI",
                    SourceType::Mp3 => "MP3",
                    SourceType::Flac => "FLAC",
                    SourceType::Video => "VIDEO",
                    SourceType::Vorbis => "VORBIS",
                    SourceType::OfflineWave => "WAVE",
                    SourceType::Section => "SECTION",
                    SourceType::Empty => "EMPTY",
                    SourceType::Unknown(ref s) => s.as_str(),
                };
                out.push_str(&format!("{}  <SOURCE {}\n", prefix, type_str));
                if !source.file_path.is_empty() {
                    out.push_str(&format!("{}    FILE {:?}\n", prefix, source.file_path));
                }
                if !source.raw_content.is_empty() {
                    for line in source.raw_content.lines() {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() && !trimmed.starts_with("FILE ") {
                            out.push_str(&format!("{}    {}\n", prefix, trimmed));
                        }
                    }
                }
                out.push_str(&format!("{}  >\n", prefix));
            }
        }
    }

    out.push_str(&format!("{}>\n", prefix));
}

// ── Raw Chunk-Based Concatenation ────────────────────────────────────────────

/// A tempo point extracted from raw RPP text.
struct RawTempoPoint {
    /// The formatted PT line (with offset applied).
    line: String,
    /// The tempo (BPM) value at this point.
    tempo: String,
}

/// Extract PT (tempo point) lines from a TEMPOENVEX block in raw RPP text,
/// applying a time offset to each point's position.
/// Returns the points and the default tempo from the TEMPO header line.
/// Extract tempo points from a TEMPOENVEX block, applying a time offset.
///
/// Points outside the `[local_start, local_end)` range are excluded.
/// This prevents tempo points from bleeding outside a song's bounds.
fn extract_tempo_points_raw(
    rpp_text: &str,
    offset: f64,
    local_start: f64,
    local_end: Option<f64>,
) -> (Vec<RawTempoPoint>, Option<String>) {
    let mut points = Vec::new();
    let mut in_tempoenvex = false;
    let mut default_tempo = None;

    for line in rpp_text.lines() {
        let trimmed = line.trim();

        // Capture the project's default TEMPO line
        if default_tempo.is_none() && trimmed.starts_with("TEMPO ") {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 2 {
                default_tempo = Some(parts[1].to_string());
            }
        }

        if trimmed.starts_with("<TEMPOENVEX") {
            in_tempoenvex = true;
            continue;
        }
        if in_tempoenvex && trimmed == ">" {
            break;
        }
        if in_tempoenvex && trimmed.starts_with("PT ") {
            // PT <position> <tempo> <shape> [optional fields...]
            let parts: Vec<&str> = trimmed.splitn(4, ' ').collect();
            if parts.len() >= 3
                && let Ok(pos) = parts[1].parse::<f64>()
            {
                // Skip points before the song's start bound
                if pos < local_start {
                    continue;
                }
                // Skip points beyond the song's end bound
                if let Some(end) = local_end
                    && pos >= end
                {
                    continue;
                }
                let new_pos = pos + offset;
                let rest = if parts.len() > 3 {
                    format!(" {}", parts[3])
                } else {
                    String::new()
                };
                points.push(RawTempoPoint {
                    line: format!("    PT {:.12} {}{}", new_pos, parts[2], rest),
                    tempo: parts[2].to_string(),
                });
            }
        }
    }
    (points, default_tempo)
}

/// Write a combined TEMPOENVEX block from all projects into the output string.
///
/// Collects tempo points from each project's TEMPOENVEX, offsets them by each
/// song's global position, and emits a single combined envelope. A square-shape
/// boundary point is inserted at each song transition.
// r[impl combine.tempo-boundary]
// r[impl combine.tempo-preserve-internal]
fn write_combined_tempoenvex(out: &mut String, rpp_paths: &[PathBuf], song_infos: &[SongInfo]) {
    out.push_str("  <TEMPOENVEX\n");
    out.push_str("    ACT 1 -1\n");
    out.push_str("    VIS 1 0 1\n");
    out.push_str("    LANEHEIGHT 0 0\n");
    out.push_str("    ARM 0\n");
    // DEFSHAPE 1 = square by default — prevents accidental gradual transitions
    // (REAPER: 0=linear, 1=square, 2=slow start/end, etc.)
    out.push_str("    DEFSHAPE 1 -1 -1\n");

    for (song_idx, (rpp_path, song)) in rpp_paths.iter().zip(song_infos.iter()).enumerate() {
        let rpp_text = match std::fs::read_to_string(rpp_path) {
            Ok(t) => t,
            Err(_) => continue,
        };

        let offset = song.global_start_seconds - song.local_start;
        let local_end = Some(song.local_start + song.duration_seconds);
        let (points, default_tempo) =
            extract_tempo_points_raw(&rpp_text, offset, song.local_start, local_end);
        let song_end = song.global_start_seconds + song.duration_seconds;
        let is_last_song = song_idx == song_infos.len() - 1;

        // Parse default BPM and time signature from TEMPO header line
        let mut bpm_str = default_tempo.unwrap_or_else(|| "120".to_string());
        let mut ts_encoded = 262148i32; // default 4/4
        for line in rpp_text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("TEMPO ") {
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 4 {
                    bpm_str = parts[1].to_string();
                    let num: i32 = parts[2].parse().unwrap_or(4);
                    let denom: i32 = parts[3].parse().unwrap_or(4);
                    ts_encoded = 65536 * denom + num;
                }
                break;
            }
        }

        // Always emit a tempo point exactly at the song's start position.
        // This ensures each song begins with the correct tempo and time
        // signature, regardless of where the first PT line falls.
        //
        // If we have tempo points AND the first one is already at the start,
        // we just force it to square shape + correct time sig. Otherwise we
        // insert a leading point from the TEMPO header.
        let first_is_at_start = points.first().is_some_and(|pt| {
            // Check if the first point is within 1ms of global_start
            let pt_pos = pt
                .line
                .split_whitespace()
                .nth(1)
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(f64::MAX);
            (pt_pos - song.global_start_seconds).abs() < 0.001
        });

        if points.is_empty() || !first_is_at_start {
            // Insert a leading tempo point at the song start
            out.push_str(&format!(
                "    PT {:.12} {} 1 {} 0 1 0 \"\" 0 169 0 ABBB\n",
                song.global_start_seconds, bpm_str, ts_encoded
            ));
        }

        // Write remaining tempo points
        for (i, pt) in points.iter().enumerate() {
            if i == 0 && first_is_at_start {
                // First point is at song start — force square + time sig
                let line = ensure_time_signature(&force_square_shape(&pt.line), ts_encoded);
                out.push_str(&line);
            } else {
                out.push_str(&pt.line);
            }
            out.push('\n');
        }

        // Insert a square boundary point at the song's end.
        // This freezes the tempo so it doesn't interpolate into the next song.
        if !is_last_song {
            let end_tempo = points.last().map(|p| p.tempo.as_str()).unwrap_or(&bpm_str);
            // Shape=1 = square (instant jump, no gradual transition)
            out.push_str(&format!("    PT {:.12} {} 1\n", song_end, end_tempo));
        }
    }

    out.push_str("  >\n");
}

/// Ensure a PT line includes the time signature in field 4.
///
/// PT format: `PT <position> <tempo> <shape> [<ts_encoded> <rest...>]`
/// If the line has only 3 fields (position, tempo, shape) or the ts field
/// is missing/zero, inject the project's time signature so the song doesn't
/// inherit the previous song's time signature.
fn ensure_time_signature(pt_line: &str, ts_encoded: i32) -> String {
    let trimmed = pt_line.trim();
    let parts: Vec<&str> = trimmed.splitn(5, ' ').collect();
    // parts: ["PT", position, tempo, shape, rest...]
    if parts.len() < 4 {
        return pt_line.to_string();
    }

    if parts.len() == 4 {
        // No time signature field — append it with standard trailing fields
        format!(
            "    PT {} {} {} {} 0 1 0 \"\" 0 169 0 ABBB",
            parts[1], parts[2], parts[3], ts_encoded
        )
    } else {
        // Has rest — check if the ts field is present and non-zero
        let rest = parts[4];
        let existing_ts: i32 = rest
            .split_whitespace()
            .next()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        if existing_ts == 0 {
            // Replace the first token in rest with the correct ts
            let after_ts = rest
                .split_whitespace()
                .skip(1)
                .collect::<Vec<_>>()
                .join(" ");
            if after_ts.is_empty() {
                format!(
                    "    PT {} {} {} {}",
                    parts[1], parts[2], parts[3], ts_encoded
                )
            } else {
                format!(
                    "    PT {} {} {} {} {}",
                    parts[1], parts[2], parts[3], ts_encoded, after_ts
                )
            }
        } else {
            // Already has a valid time signature — leave as-is
            pt_line.to_string()
        }
    }
}

/// Force a PT line's shape field to 1 (square / no gradual transition).
///
/// PT lines have format: `    PT <position> <tempo> <shape> [rest...]`
/// REAPER shape values: 0=linear, 1=square, 2=slow start/end, etc.
/// We replace the shape field with 1 (square).
fn force_square_shape(pt_line: &str) -> String {
    let trimmed = pt_line.trim();
    let parts: Vec<&str> = trimmed.splitn(5, ' ').collect();
    // parts: ["PT", position, tempo, shape, rest...]
    if parts.len() >= 4 {
        let rest = if parts.len() > 4 {
            format!(" {}", parts[4])
        } else {
            String::new()
        };
        format!("    PT {} {} 1{}", parts[1], parts[2], rest)
    } else {
        pt_line.to_string()
    }
}

/// Concatenate multiple RPP files by directly manipulating the raw text.
///
/// This preserves ALL data (FX, MIDI, envelopes, takes, sources, fades, etc.)
/// by using the original RPP text and only patching POSITION and FILE lines.
// r[impl combine.raw-pipeline]
// r[impl combine.file-paths]
// r[impl combine.markers]
// r[impl combine.song-folders]
// r[impl combine.tempo-concat]
pub fn concatenate_rpp_files_raw(rpp_paths: &[PathBuf], song_infos: &[SongInfo]) -> String {
    assert_eq!(rpp_paths.len(), song_infos.len());

    let mut out = String::new();

    // Write project header from the first project
    let first_rpp = std::fs::read_to_string(&rpp_paths[0]).unwrap_or_default();

    // Extract header (everything before first <TRACK)
    let first_track_idx = first_rpp.find("<TRACK").unwrap_or(first_rpp.len());
    let header = &first_rpp[..first_track_idx];

    // Write header, skipping MARKER lines and the TEMPOENVEX block
    // (we'll write our own combined versions below)
    let mut in_tempoenvex = false;
    let mut tempoenvex_depth = 0;
    for line in header.lines() {
        let trimmed = line.trim();

        // Track TEMPOENVEX block depth to skip the entire block
        if trimmed.starts_with("<TEMPOENVEX") {
            in_tempoenvex = true;
            tempoenvex_depth = 1;
            continue;
        }
        if in_tempoenvex {
            if trimmed.starts_with('<') {
                tempoenvex_depth += 1;
            }
            if trimmed == ">" {
                tempoenvex_depth -= 1;
                if tempoenvex_depth == 0 {
                    in_tempoenvex = false;
                }
            }
            continue;
        }

        // Skip markers (we'll add our own)
        if trimmed.starts_with("MARKER ") {
            continue;
        }
        // Skip existing ruler lines (we'll write our own)
        if trimmed.starts_with("RULERHEIGHT ") || trimmed.starts_with("RULERLANE ") {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }

    // Write combined tempo envelope from all projects
    write_combined_tempoenvex(&mut out, rpp_paths, song_infos);

    // Write ruler lane definitions
    out.push_str("  RULERHEIGHT 120 84\n");
    out.push_str("  RULERLANE 1 8 SECTIONS 0 -1\n");
    out.push_str("  RULERLANE 2 0 MARKS 0 -1\n");
    out.push_str("  RULERLANE 3 4 SONG 0 -1\n");
    out.push_str("  RULERLANE 4 0 START/END 0 -1\n");
    out.push_str("  RULERLANE 5 0 KEY 0 -1\n");
    out.push_str("  RULERLANE 6 0 MODE 0 -1\n");
    out.push_str("  RULERLANE 7 0 CHORDS 0 -1\n");
    out.push_str("  RULERLANE 8 0 NOTES 0 -1\n");

    // Process each song
    for (song_idx, (rpp_path, song)) in rpp_paths.iter().zip(song_infos.iter()).enumerate() {
        let rpp_text = match std::fs::read_to_string(rpp_path) {
            Ok(t) => t,
            Err(_) => continue,
        };

        let offset = song.global_start_seconds - song.local_start;
        let source_dir = rpp_path.parent().unwrap_or(Path::new("."));

        // Write song region marker (SONG lane = 3)
        let song_end = song.global_start_seconds + song.duration_seconds + 0.1;
        out.push_str(&format!(
            "  MARKER {} {} {:?} 1 0 1 R {{}} 0 3\n",
            song_idx * 100 + 1,
            song.global_start_seconds,
            song.name
        ));
        out.push_str(&format!(
            "  MARKER {} {} \"\" 1 0 1 R {{}} 0 3\n",
            song_idx * 100 + 1,
            song_end
        ));

        // Write offset markers from this project
        for line in rpp_text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("MARKER ")
                && let Some(patched) = offset_marker_line(trimmed, offset)
            {
                let lane = classify_marker_lane_from_line(trimmed);
                // Rewrite with lane
                out.push_str(&format!("  {} {}\n", patched, lane));
            }
        }

        // Write song folder track
        out.push_str("  <TRACK {}\n");
        out.push_str(&format!("    NAME {:?}\n", song.name));
        out.push_str("    ISBUS 1 0\n");
        out.push_str("    NCHAN 2\n");
        out.push_str("  >\n");

        // Extract and write all TRACK blocks with offset positions and resolved paths
        let track_blocks = extract_track_blocks(&rpp_text);
        let total_tracks = track_blocks.len();

        for (t_idx, block) in track_blocks.iter().enumerate() {
            let patched = patch_track_block(block, offset, source_dir);

            // Last track in song needs to close the song folder
            if t_idx == total_tracks - 1 {
                // Replace the last ISBUS line or add folder close
                let patched = close_song_folder(&patched);
                out.push_str(&patched);
            } else {
                out.push_str(&patched);
            }
        }
    }

    // Close the project
    out.push_str(">\n");
    out
}

/// Extract all <TRACK ...> ... > blocks from RPP text.
fn extract_track_blocks(rpp: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut depth = 0;
    let mut current_block = String::new();
    let mut in_track = false;

    for line in rpp.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("<TRACK") && depth == 1 {
            // Top-level track (depth 1 = inside project)
            in_track = true;
            current_block.clear();
            current_block.push_str(line);
            current_block.push('\n');
            depth += 1;
            continue;
        }

        if trimmed.starts_with('<') && !trimmed.starts_with("<!") {
            depth += 1;
        }

        if in_track {
            current_block.push_str(line);
            current_block.push('\n');
        }

        if trimmed == ">" {
            depth -= 1;
            if in_track && depth <= 1 {
                blocks.push(current_block.clone());
                current_block.clear();
                in_track = false;
            }
        }
    }

    blocks
}

/// Patch a track block: offset POSITION lines, resolve FILE paths.
/// Parse a quoted file path from a FILE line's value portion.
///
/// Handles `"path/to/file.wav"`, `"path/to/file.wav" 1`, and unquoted paths.
/// Returns `(path, trailing)` where trailing is any text after the closing quote.
fn parse_quoted_file_path(s: &str) -> Option<(String, &str)> {
    let s = s.trim();
    if let Some(stripped) = s.strip_prefix('"') {
        // Find the closing quote
        if let Some(end) = stripped.find('"') {
            let path = &stripped[..end];
            let trailing = stripped[end + 1..].trim();
            Some((path.to_string(), trailing))
        } else {
            // No closing quote — take everything after the opening quote
            Some((stripped.to_string(), ""))
        }
    } else {
        // Unquoted — take the first whitespace-delimited token
        let (path, rest) = s.split_once(char::is_whitespace).unwrap_or((s, ""));
        Some((path.to_string(), rest.trim()))
    }
}

fn patch_track_block(block: &str, offset: f64, source_dir: &Path) -> String {
    let mut result = String::new();

    for line in block.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("POSITION ") {
            // Offset the position
            if let Some(pos_str) = trimmed.strip_prefix("POSITION ")
                && let Ok(pos) = pos_str.trim().parse::<f64>()
            {
                let new_pos = pos + offset;
                if new_pos < -0.01 {
                    // Item before song bounds — skip entire item
                    // (we'd need to skip until matching >, but for now just set to 0)
                }
                // Preserve indentation
                let indent = line.len() - line.trim_start().len();
                result.push_str(&" ".repeat(indent));
                result.push_str(&format!("POSITION {}\n", new_pos));
                continue;
            }
        }

        if trimmed.starts_with("FILE ") {
            // Resolve relative paths to absolute.
            // FILE lines can be: FILE "path" or FILE "path" 1 (with trailing flags)
            let after_file = trimmed.strip_prefix("FILE ").unwrap_or("");
            if let Some((file_path, trailing)) = parse_quoted_file_path(after_file)
                && !file_path.is_empty()
                && !PathBuf::from(&file_path).is_absolute()
            {
                let absolute = source_dir.join(&file_path);
                let indent = line.len() - line.trim_start().len();
                result.push_str(&" ".repeat(indent));
                result.push_str(&format!("FILE \"{}\"", absolute.to_string_lossy()));
                if !trailing.is_empty() {
                    result.push(' ');
                    result.push_str(trailing);
                }
                result.push('\n');
                continue;
            }
        }

        result.push_str(line);
        result.push('\n');
    }

    result
}

/// Make the last track in a song folder close the folder (ISBUS 2 -1).
fn close_song_folder(block: &str) -> String {
    // Find the last ISBUS line and replace it
    let lines: Vec<&str> = block.lines().collect();
    let mut result = String::new();
    let mut last_isbus_idx = None;

    for (i, line) in lines.iter().enumerate() {
        if line.trim().starts_with("ISBUS ") {
            last_isbus_idx = Some(i);
        }
    }

    for (i, line) in lines.iter().enumerate() {
        if Some(i) == last_isbus_idx {
            let indent = line.len() - line.trim_start().len();
            result.push_str(&" ".repeat(indent));
            result.push_str("ISBUS 2 -1\n");
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }

    result
}

/// Offset a MARKER line's position value.
fn offset_marker_line(line: &str, offset: f64) -> Option<String> {
    // MARKER id position "name" ...
    let parts: Vec<&str> = line.splitn(4, ' ').collect();
    if parts.len() < 3 {
        return None;
    }
    let pos: f64 = parts[2].parse().ok()?;
    let new_pos = pos + offset;
    let rest = if parts.len() > 3 {
        parts[3..].join(" ")
    } else {
        String::new()
    };
    Some(format!("MARKER {} {} {}", parts[1], new_pos, rest))
}

/// Classify a marker line to determine its ruler lane.
fn classify_marker_lane_from_line(line: &str) -> u32 {
    let upper = line.to_uppercase();
    if upper.contains("SONGSTART")
        || upper.contains("SONGEND")
        || upper.contains("COUNT-IN")
        || upper.contains("COUNTIN")
    {
        2 // MARKS
    } else if upper.contains("=START")
        || upper.contains("=END")
        || upper.contains("PREROLL")
        || upper.contains("POSTROLL")
    {
        4 // START/END
    } else if line.contains("\" 1 0") {
        // Regions (have flag 1 after name) go to SECTIONS
        1
    } else {
        0 // default
    }
}

// ── Lane Classification ──────────────────────────────────────────────────────

/// FTS ruler lane indices (matching session-proto::ruler_lanes::CoreLane).
const LANE_SECTIONS: u32 = 1;
const LANE_MARKS: u32 = 2;
const LANE_SONG: u32 = 3;
const LANE_START_END: u32 = 4;

/// Classify a marker/region name into the correct FTS ruler lane index.
fn classify_lane(name: &str, is_region: bool) -> u32 {
    let upper = name.trim().to_uppercase();

    // Structural markers → MARKS lane
    match upper.as_str() {
        "SONGSTART" | "SONGEND" | "COUNT-IN" | "COUNT IN" | "COUNTIN" => return LANE_MARKS,
        "=START" | "=END" | "PREROLL" | "=PREROLL" | "POSTROLL" | "=POSTROLL" => {
            return LANE_START_END;
        }
        _ => {}
    }
    if name.starts_with('=') {
        return LANE_START_END;
    }

    if is_region {
        // Regions go to SECTIONS lane (Verse, Chorus, Intro, etc.)
        LANE_SECTIONS
    } else {
        // Unclassified markers go to default lane
        0
    }
}

/// Organize marker/region lanes in raw RPP text without parsing/re-serializing.
///
/// This patches each MARKER line's trailing lane number based on the marker name,
/// preserving all other fields (color, GUID, flags, etc.) exactly as-is.
/// Also ensures the standard FTS RULERLANE definitions are present.
///
/// This is the safe alternative to `organize_ruler_lanes` — it avoids the lossy
/// parse→re-serialize cycle that strips colors, GUIDs, and flags.
pub fn organize_marker_lanes_raw(rpp_text: &str) -> String {
    let mut result = String::with_capacity(rpp_text.len());
    let mut ruler_lanes_written = false;

    for line in rpp_text.lines() {
        let trimmed = line.trim();

        // Skip existing RULERLANE lines — we'll write our own
        if trimmed.starts_with("RULERLANE ") {
            if !ruler_lanes_written {
                // Write FTS standard ruler lanes (once, replacing all originals)
                result.push_str("  RULERLANE 1 8 SECTIONS 0 -1\n");
                result.push_str("  RULERLANE 2 0 MARKS 0 -1\n");
                result.push_str("  RULERLANE 3 4 SONG 0 -1\n");
                result.push_str("  RULERLANE 4 0 START/END 0 -1\n");
                result.push_str("  RULERLANE 5 0 KEY 0 -1\n");
                result.push_str("  RULERLANE 6 0 MODE 0 -1\n");
                result.push_str("  RULERLANE 7 0 CHORDS 0 -1\n");
                result.push_str("  RULERLANE 8 0 NOTES 0 -1\n");
                ruler_lanes_written = true;
            }
            continue;
        }

        // Patch MARKER lines: replace the trailing lane number
        if trimmed.starts_with("MARKER ") {
            let lane = classify_marker_lane_for_raw(trimmed);
            // The lane is the last token on the line. Replace it.
            if let Some(last_space) = trimmed.rfind(' ') {
                let prefix = &trimmed[..last_space];
                result.push_str(&format!("  {} {}\n", prefix, lane));
            } else {
                result.push_str(line);
                result.push('\n');
            }
            continue;
        }

        result.push_str(line);
        result.push('\n');
    }

    result
}

/// Classify a raw MARKER line into the correct FTS ruler lane.
///
/// Extracts the marker name from the line and classifies it.
/// Returns the lane index to use.
fn classify_marker_lane_for_raw(marker_line: &str) -> u32 {
    // MARKER format: MARKER id position "name" flags color isrgn type {guid} additional [lane]
    // Or for region end: MARKER id position "" flags ...
    //
    // Extract the name (quoted or unquoted after position)
    let name = extract_marker_name(marker_line);

    // Empty name = region end line — keep on same lane as its pair
    // We can't know the pair's lane here, so use 0 (will inherit from pair)
    if name.is_empty() {
        // Check if this line already has a lane we should preserve
        // For region end markers, the lane should match the start marker
        // The raw combine already sets the correct lane, so preserve it
        return extract_trailing_lane(marker_line);
    }

    // Check if name matches a structural marker
    let upper = name.to_uppercase();
    match upper.as_str() {
        "SONGSTART" | "SONGEND" | "COUNT-IN" | "COUNT IN" | "COUNTIN" => return LANE_MARKS,
        "=START" | "=END" | "PREROLL" | "=PREROLL" | "POSTROLL" | "=POSTROLL" => {
            return LANE_START_END;
        }
        _ => {}
    }
    if name.starts_with('=') {
        return LANE_START_END;
    }

    // Check if this is a region (flag field contains 1 after the name)
    let is_region = is_region_marker_line(marker_line);
    if is_region {
        // Check if this was already on the SONG lane (song-spanning regions
        // generated by the combine pipeline should stay there)
        let current_lane = extract_trailing_lane(marker_line);
        if current_lane == LANE_SONG {
            return LANE_SONG;
        }
        return LANE_SECTIONS;
    }

    // Unclassified marker — keep current lane or use 0
    extract_trailing_lane(marker_line)
}

/// Extract the marker name from a MARKER line.
fn extract_marker_name(line: &str) -> String {
    // Find quoted name: MARKER id pos "name" ...
    if let Some(quote_start) = line.find('"')
        && let Some(quote_end) = line[quote_start + 1..].find('"')
    {
        return line[quote_start + 1..quote_start + 1 + quote_end].to_string();
    }
    // Unquoted name: MARKER id pos name ...
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() >= 4 {
        parts[3].to_string()
    } else {
        String::new()
    }
}

/// Check if a MARKER line represents a region (has flag 1 after name).
fn is_region_marker_line(line: &str) -> bool {
    // After the name, regions have "1" as the flags field
    // Format: MARKER id pos "name" 1 color 1 R {guid} ...
    // vs markers: MARKER id pos "name" 0 color 1 B {guid} ...
    // The "R" vs "B" after "1" distinguishes regions from markers
    line.contains(" R {") || line.contains(" R {")
}

/// Extract the trailing lane number from a MARKER line.
fn extract_trailing_lane(line: &str) -> u32 {
    line.split_whitespace()
        .last()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0)
}

/// Organize all markers and regions in a project into the correct FTS ruler lanes.
///
/// This reclassifies every marker/region based on its name, overriding any
/// existing lane assignment. Call this on the combined project after
/// concatenation, or on individual projects for offline cleanup.
///
/// Also ensures the standard FTS ruler lane definitions are present.
pub fn organize_ruler_lanes(project: &mut ReaperProject) {
    // Set standard ruler lanes
    use crate::types::project::RulerLane;
    let fts_lane = |index, flags, name: &str| RulerLane {
        index,
        flags,
        name: name.to_string(),
        color: 0,
        extra: -1,
    };
    project.ruler_lanes = vec![
        fts_lane(1, 8, "SECTIONS"), // flag 8 = default region lane
        fts_lane(2, 0, "MARKS"),
        fts_lane(3, 4, "SONG"), // flag 4 = default marker lane
        fts_lane(4, 0, "START/END"),
        fts_lane(5, 0, "KEY"),
        fts_lane(6, 0, "MODE"),
        fts_lane(7, 0, "CHORDS"),
        fts_lane(8, 0, "NOTES"),
    ];

    // Reclassify all markers and regions.
    // Preserve SONG lane (3) for markers already assigned there
    // (these are the song-spanning regions generated by the combine pipeline).
    for mr in &mut project.markers_regions.all {
        if mr.lane == Some(LANE_SONG as i32) {
            // Already on SONG lane — keep it there
            continue;
        }
        let lane = classify_lane(&mr.name, mr.is_region());
        mr.lane = Some(lane as i32);
    }
    // Update filtered views
    project.markers_regions.markers = project
        .markers_regions
        .all
        .iter()
        .filter(|m| m.is_marker())
        .cloned()
        .collect();
    project.markers_regions.regions = project
        .markers_regions
        .all
        .iter()
        .filter(|m| m.is_region())
        .cloned()
        .collect();
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_item(position: f64, length: f64) -> Item {
        Item {
            position,
            length,
            ..Item::default()
        }
    }

    fn make_project_with_tracks(tracks: Vec<Track>) -> ReaperProject {
        ReaperProject {
            tracks,
            ..ReaperProject::default()
        }
    }

    fn make_empty_project() -> ReaperProject {
        ReaperProject::default()
    }

    fn make_songs(entries: &[(&str, f64)]) -> Vec<SongInfo> {
        let mut offset = 0.0;
        entries
            .iter()
            .map(|(name, dur)| {
                let s = SongInfo {
                    name: name.to_string(),
                    global_start_seconds: offset,
                    duration_seconds: *dur,
                    local_start: 0.0,
                    source_dir: None,
                };
                offset += dur;
                s
            })
            .collect()
    }

    #[test]
    fn guide_tracks_merged_with_offsets() {
        let p1 = make_project_with_tracks(vec![
            {
                let mut t = empty_track("Click");
                t.items = vec![make_item(0.0, 1.0)];
                t
            },
            {
                let mut t = empty_track("Guitar");
                t.items = vec![make_item(0.0, 10.0)];
                t
            },
        ]);
        let p2 = make_project_with_tracks(vec![
            {
                let mut t = empty_track("Click");
                t.items = vec![make_item(0.0, 1.0)];
                t
            },
            {
                let mut t = empty_track("Bass");
                t.items = vec![make_item(0.0, 15.0)];
                t
            },
        ]);

        let songs = make_songs(&[("Song A", 30.0), ("Song B", 45.0)]);
        let tracks = concatenate_tracks(&[p1, p2], &songs);

        let click = tracks.iter().find(|t| t.name == "Click").unwrap();
        assert_eq!(click.items.len(), 2);
        assert_eq!(click.items[0].position, 0.0);
        assert_eq!(click.items[1].position, 30.0);
    }

    #[test]
    fn content_tracks_offset_in_song_folders() {
        let p1 = make_project_with_tracks(vec![{
            let mut t = empty_track("Guitar");
            t.items = vec![make_item(0.0, 10.0)];
            t
        }]);
        let p2 = make_project_with_tracks(vec![{
            let mut t = empty_track("Bass");
            t.items = vec![make_item(0.0, 15.0)];
            t
        }]);

        let songs = make_songs(&[("Song A", 30.0), ("Song B", 45.0)]);
        let tracks = concatenate_tracks(&[p1, p2], &songs);

        let names: Vec<&str> = tracks.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"TRACKS"));
        assert!(names.contains(&"Song A"));
        assert!(names.contains(&"Song B"));

        let bass = tracks.iter().find(|t| t.name == "Bass").unwrap();
        assert_eq!(bass.items[0].position, 30.0);
    }

    #[test]
    fn tempo_points_offset_and_square_boundary() {
        let mut p1 = make_empty_project();
        p1.tempo_envelope = Some(TempoTimeEnvelope {
            points: vec![TempoTimePoint {
                position: 0.0,
                tempo: 120.0,
                shape: 0,
                time_signature_encoded: None,
                selected: false,
                unknown1: 0,
                bezier_tension: 0.0,
                metronome_pattern: String::new(),
                unknown2: 0,
                unknown3: 0,
                unknown4: 0,
            }],
            default_tempo: 120.0,
            default_time_signature: (4, 4),
        });

        let mut p2 = make_empty_project();
        p2.tempo_envelope = Some(TempoTimeEnvelope {
            points: vec![TempoTimePoint {
                position: 0.0,
                tempo: 90.0,
                shape: 0,
                time_signature_encoded: None,
                selected: false,
                unknown1: 0,
                bezier_tension: 0.0,
                metronome_pattern: String::new(),
                unknown2: 0,
                unknown3: 0,
                unknown4: 0,
            }],
            default_tempo: 90.0,
            default_time_signature: (3, 4),
        });

        let songs = make_songs(&[("A", 30.0), ("B", 45.0)]);
        let envelope = concatenate_tempo_envelopes(&[p1, p2], &songs);

        // Song A: leading at 0.0 (120bpm) + trailing at 30.0 (120bpm)
        // Song B: leading at 30.0 (90bpm) + trailing at 75.0 (90bpm)
        assert_eq!(envelope.points.len(), 4);
        assert_eq!(envelope.points[0].position, 0.0);
        assert_eq!(envelope.points[0].tempo, 120.0);
        assert_eq!(envelope.points[1].position, 30.0);
        assert_eq!(
            envelope.points[1].tempo, 120.0,
            "Trailing marker freezes song A tempo"
        );
        assert_eq!(envelope.points[2].position, 30.0);
        assert_eq!(envelope.points[2].tempo, 90.0);
        assert_eq!(envelope.points[2].shape, 1, "Boundary should be square");
        assert_eq!(envelope.points[3].position, 75.0);
        assert_eq!(
            envelope.points[3].tempo, 90.0,
            "Trailing marker freezes song B tempo"
        );
    }

    #[test]
    fn song_regions_generated_at_correct_positions() {
        let p1 = make_empty_project();
        let p2 = make_empty_project();

        let songs = make_songs(&[("Song A", 30.0), ("Song B", 45.0)]);
        let collection = generate_markers_regions(&[p1, p2], &songs);

        assert!(collection.regions.len() >= 2);

        let ra = collection
            .regions
            .iter()
            .find(|r| r.name == "Song A")
            .unwrap();
        assert_eq!(ra.position, 0.0);
        assert_eq!(ra.end_position, Some(30.0));

        let rb = collection
            .regions
            .iter()
            .find(|r| r.name == "Song B")
            .unwrap();
        assert_eq!(rb.position, 30.0);
        assert_eq!(rb.end_position, Some(75.0));
    }

    #[test]
    fn internal_markers_offset_and_ids_unique() {
        let mut p1 = make_empty_project();
        p1.markers_regions.all.push(MarkerRegion {
            id: 1,
            position: 5.0,
            name: "Chorus".to_string(),
            color: 0,
            flags: 0,
            locked: 0,
            guid: "old".to_string(),
            additional: 0,
            end_position: Some(15.0),
            lane: None,
            beat_position: None,
        });

        let mut p2 = make_empty_project();
        p2.markers_regions.all.push(MarkerRegion {
            id: 1, // Same ID as p1
            position: 2.0,
            name: "Verse".to_string(),
            color: 0,
            flags: 0,
            locked: 0,
            guid: "old2".to_string(),
            additional: 0,
            end_position: None,
            lane: None,
            beat_position: None,
        });

        let songs = make_songs(&[("Song A", 30.0), ("Song B", 45.0)]);
        let collection = generate_markers_regions(&[p1, p2], &songs);

        // Chorus should be at position 5 (Song A offset = 0)
        let chorus = collection.all.iter().find(|m| m.name == "Chorus").unwrap();
        assert_eq!(chorus.position, 5.0);
        assert!(chorus.guid.is_empty());

        // Verse should be at position 32 (Song B offset = 30 + 2)
        let verse = collection.all.iter().find(|m| m.name == "Verse").unwrap();
        assert_eq!(verse.position, 32.0);

        // All IDs unique
        let ids: Vec<i32> = collection.all.iter().map(|m| m.id).collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len(), "IDs must be unique");
    }

    // ── Lane classification tests ────────────────────────────────────────

    #[test]
    fn organize_lanes_raw_preserves_marker_data() {
        // A minimal RPP with markers that have color, GUID, and flags
        let input = r#"<REAPER_PROJECT 0.1 "7.0/generated" 0
  TEMPO 120 4 4 0
  RULERLANE 1 0 Default 0 -1
  MARKER 1 0 "Intro" 1 16777215 1 R {AAAA-BBBB} 0 0
  MARKER 1 10 "" 1 0 1 R {AAAA-BBBB} 0 0
  MARKER 2 10 SONGSTART 0 12345 1 B {CCCC-DDDD} 0 0
  MARKER 3 50 "=END" 0 0 1 B {EEEE-FFFF} 0 0
  MARKER 4 0 "Belief" 1 0 1 R {} 0 3
  MARKER 4 60 "" 1 0 1 R {} 0 3
>"#;
        let output = organize_marker_lanes_raw(input);

        // Should have FTS ruler lanes
        assert!(
            output.contains("RULERLANE 1 8 SECTIONS"),
            "Missing SECTIONS lane"
        );
        assert!(output.contains("RULERLANE 2 0 MARKS"), "Missing MARKS lane");
        assert!(output.contains("RULERLANE 3 4 SONG"), "Missing SONG lane");
        assert!(
            output.contains("RULERLANE 4 0 START/END"),
            "Missing START/END lane"
        );

        // Should NOT have old "Default" lane
        assert!(!output.contains("Default"), "Old lane should be replaced");

        // Intro region → SECTIONS (lane 1), preserving color and GUID
        assert!(
            output.contains("\"Intro\" 1 16777215 1 R {AAAA-BBBB} 0 1"),
            "Intro should be lane 1 (SECTIONS) with original color/GUID preserved.\nGot: {}",
            output
                .lines()
                .find(|l| l.contains("Intro"))
                .unwrap_or("NOT FOUND")
        );

        // SONGSTART → MARKS (lane 2), preserving color
        assert!(
            output.contains("SONGSTART 0 12345 1 B {CCCC-DDDD} 0 2"),
            "SONGSTART should be lane 2 (MARKS) with original data.\nGot: {}",
            output
                .lines()
                .find(|l| l.contains("SONGSTART"))
                .unwrap_or("NOT FOUND")
        );

        // =END → START/END (lane 4)
        assert!(
            output.contains("\"=END\" 0 0 1 B {EEEE-FFFF} 0 4"),
            "=END should be lane 4 (START/END).\nGot: {}",
            output
                .lines()
                .find(|l| l.contains("=END"))
                .unwrap_or("NOT FOUND")
        );

        // Song region should stay on SONG lane (3)
        assert!(
            output.contains("\"Belief\" 1 0 1 R {} 0 3"),
            "Song region should stay on lane 3 (SONG).\nGot: {}",
            output
                .lines()
                .find(|l| l.contains("Belief"))
                .unwrap_or("NOT FOUND")
        );
    }

    #[test]
    fn organize_lanes_raw_section_regions_go_to_sections() {
        let input = r#"<REAPER_PROJECT 0.1 "7.0" 0
  MARKER 1 0 "VS 1" 1 0 1 R {} 0 0
  MARKER 1 10 "" 1 0 1 R {} 0 0
  MARKER 2 10 "Chorus" 1 0 1 R {} 0 0
  MARKER 2 20 "" 1 0 1 R {} 0 0
  MARKER 3 20 "Bridge" 1 0 1 R {} 0 0
  MARKER 3 30 "" 1 0 1 R {} 0 0
>"#;
        let output = organize_marker_lanes_raw(input);

        // All section regions should be on lane 1
        for name in &["VS 1", "Chorus", "Bridge"] {
            let line = output.lines().find(|l| l.contains(name)).expect(name);
            assert!(
                line.ends_with(" 1"),
                "{} should be on lane 1, got: {}",
                name,
                line
            );
        }
    }

    #[test]
    fn extract_marker_name_works() {
        assert_eq!(
            extract_marker_name(r#"MARKER 1 0 "Intro" 1 0 1 R {} 0 1"#),
            "Intro"
        );
        assert_eq!(
            extract_marker_name("MARKER 2 10 SONGSTART 0 0 1 B {} 0 2"),
            "SONGSTART"
        );
        assert_eq!(extract_marker_name(r#"MARKER 1 10 "" 1"#), "");
    }
}

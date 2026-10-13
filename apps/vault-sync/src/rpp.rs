//! Builds a real REAPER project per song: one track per stem, laid out
//! flat, then run through dynamic-template's real organize pipeline —
//! the exact sequence `dynamic-template --apply-buses` runs
//! (`apply_colors` → `apply_buses` → `apply_routing` → `gather_unsorted`,
//! preceded by folder-depth repair and the same classify-then-justify-a-
//! bus pass `main.rs`'s `apply_buses_to_rpp` uses) — so the result reads
//! like a hand-built session: grouped by instrument, routed to buses,
//! coloured by classification. Not a fork of that logic — every function
//! called here is the same public API `dynamic-template`'s own CLI calls.
//!
//! Reference shape: `Thank God I'm Free - Elevation Rhythm/Thank God I'm
//! Free - Elevation Worship.RPP`, a real 36-track project someone built
//! by hand for exactly this song. This produces the same kind of
//! artifact for any song in the library, from its stems alone.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use dawfile_reaper::builder::ReaperProjectBuilder;
use dawfile_reaper::{RppSerialize, SourceType};
use dynamic_template::apply::dawfile::RppTarget;
use dynamic_template::apply::{
    apply_colors, apply_routing, contextual_paths, gather_unsorted, normalize_folder_depths,
    reclassify_stem_splits,
};
use dynamic_template::buses::is_bus_name;
use dynamic_template::{TemplateTarget as _, apply_buses, bus_for_path, buses_for_paths};

use crate::library::LibrarySong;

pub fn source_type_for(path: &Path) -> SourceType {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .as_deref()
    {
        Some("ogg") => SourceType::Vorbis,
        Some("mp3") => SourceType::Mp3,
        Some("flac") => SourceType::Flac,
        _ => SourceType::Wave,
    }
}

/// The `audio/wav/`-sibling of a stem path (`audio/ogg/Foo.ogg` →
/// `audio/wav/Foo.wav`), used to probe real duration even when the item
/// itself points at the `.ogg` copy — a WAV header is trivial to read
/// without a decoder; an OGG one isn't.
pub fn wav_sibling(path: &Path) -> PathBuf {
    if path.extension().and_then(|e| e.to_str()) == Some("wav") {
        return path.to_path_buf();
    }
    let mut rebuilt = PathBuf::new();
    for component in path.components() {
        if component.as_os_str() == "ogg" {
            rebuilt.push("wav");
        } else {
            rebuilt.push(component.as_os_str());
        }
    }
    rebuilt.with_extension("wav")
}

/// Read just enough of a WAV file (the `fmt ` chunk, and the `data`
/// chunk's declared size — never its audio payload) to compute duration.
pub fn wav_duration_seconds(path: &Path) -> Option<f64> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut riff_header = [0u8; 12];
    file.read_exact(&mut riff_header).ok()?;
    if &riff_header[0..4] != b"RIFF" || &riff_header[8..12] != b"WAVE" {
        return None;
    }

    let (mut sample_rate, mut channels, mut bits_per_sample, mut data_len) =
        (0u32, 0u16, 0u16, 0u32);
    loop {
        let mut chunk_header = [0u8; 8];
        if file.read_exact(&mut chunk_header).is_err() {
            break;
        }
        let chunk_id = &chunk_header[0..4];
        let chunk_size = u32::from_le_bytes(chunk_header[4..8].try_into().ok()?);

        if chunk_id == b"fmt " {
            let mut body = vec![0u8; chunk_size as usize];
            file.read_exact(&mut body).ok()?;
            if body.len() >= 16 {
                channels = u16::from_le_bytes(body[2..4].try_into().ok()?);
                sample_rate = u32::from_le_bytes(body[4..8].try_into().ok()?);
                bits_per_sample = u16::from_le_bytes(body[14..16].try_into().ok()?);
            }
        } else if chunk_id == b"data" {
            data_len = chunk_size;
            break;
        } else {
            file.seek(SeekFrom::Current(chunk_size as i64)).ok()?;
        }
        if chunk_size % 2 == 1 {
            file.seek(SeekFrom::Current(1)).ok()?;
        }
    }

    if sample_rate == 0 || channels == 0 || bits_per_sample == 0 {
        return None;
    }
    let bytes_per_frame = channels as u64 * (bits_per_sample as u64 / 8);
    if bytes_per_frame == 0 {
        return None;
    }
    Some(data_len as f64 / (sample_rate as f64 * bytes_per_frame as f64))
}

/// Fallback item length when a stem's duration can't be probed (no WAV
/// sibling found, or an unreadable header) — generous enough that a
/// normal backing track isn't truncated; `LOOP`ed so a shorter one just
/// repeats rather than leaving silence.
pub const FALLBACK_LENGTH_SECONDS: f64 = 600.0;

/// Parse `"72bpm 4/4 #Bb"` / `"#A 127bpm 4/4"` (order-independent) from a
/// keyflow chart's second line into `(bpm, time_sig_numerator,
/// time_sig_denominator)`. Defaults to 120bpm 4/4 if the chart is absent
/// or the line doesn't parse — a wrong-tempo grid beats refusing to build
/// a project at all.
pub fn tempo_from_chart(chart: &str) -> (f64, i32, i32) {
    let mut bpm = 120.0;
    let (mut num, mut den) = (4, 4);
    if let Some(line) = chart.lines().nth(1) {
        for token in line.split_whitespace() {
            if let Some(digits) = token.strip_suffix("bpm") {
                if let Ok(parsed) = digits.parse() {
                    bpm = parsed;
                }
            } else if let Some((n, d)) = token.split_once('/')
                && let (Ok(n), Ok(d)) = (n.parse(), d.parse())
            {
                num = n;
                den = d;
            }
        }
    }
    (bpm, num, den)
}

/// Build a flat "one track per stem" project, then run it through
/// dynamic-template's organize pipeline, and return the finished RPP
/// text ready to write to disk.
pub fn build_organized_rpp(song: &LibrarySong) -> Result<String, Box<dyn std::error::Error>> {
    let (bpm, num, den) = song
        .chart_kf
        .as_deref()
        .map(tempo_from_chart)
        .unwrap_or((120.0, 4, 4));

    let mut builder = ReaperProjectBuilder::new().tempo_with_time_sig(bpm, num, den);
    for stem in &song.stems {
        let length =
            wav_duration_seconds(&wav_sibling(&stem.path)).unwrap_or(FALLBACK_LENGTH_SECONDS);
        let track_name = format!("{} - {}", song.title, stem.label);
        let source_type = source_type_for(&stem.path);
        // Relative to the RPP's own location (the song's folder), not
        // absolute — the whole folder, RPP included, is meant to be
        // portable; an absolute path would break the moment it moves.
        let file_path = stem
            .path
            .strip_prefix(&song.folder)
            .unwrap_or(&stem.path)
            .to_string_lossy()
            .into_owned();
        let item_name = stem
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_string)
            .unwrap_or_else(|| track_name.clone());
        builder = builder.track(track_name, move |t| {
            t.item(0.0, length, move |i| {
                i.take(file_path, source_type).take_name(item_name).looped()
            })
        });
    }
    let mut project = builder.build();

    // -- organize: mirrors `dynamic-template --apply-buses` exactly --
    let mut group_paths: Vec<Vec<String>> = Vec::new();
    {
        let probe = RppTarget::new(&mut project);
        for entry in reclassify_stem_splits(contextual_paths(&probe)) {
            if is_bus_name(&entry.name) {
                continue;
            }
            if bus_for_path(&entry.path).is_some() {
                group_paths.push(entry.path);
            }
        }
    }
    let buses = buses_for_paths(group_paths.iter().map(Vec::as_slice));

    let mut target = RppTarget::new(&mut project);
    normalize_folder_depths(&mut target)?;
    apply_colors(&mut target)?;
    let applied = apply_buses(&mut target, &buses)?;
    let routing = apply_routing(&mut target, &applied)?;
    let unrouted_ids: Vec<usize> = routing
        .unrouted
        .iter()
        .filter_map(|name| target.find_track(name))
        .collect();
    if !unrouted_ids.is_empty() {
        gather_unsorted(&mut target, &unrouted_ids)?;
    }

    Ok(project.to_rpp_string())
}

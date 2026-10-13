//! Offline guide track generation — generates Click, Count, and Guide MIDI items
//! from a parsed project's regions and tempo map.
//!
//! Produces MIDI items that can be inserted into the Click + Guide folder tracks
//! without requiring a running REAPER instance.

use crate::types::item::Item;
use crate::types::project::ReaperProject;
use crate::types::time_tempo::TempoTimeEnvelope;
use crate::types::track::Track;

const TICKS_PER_QN: u32 = 960;

/// Generate guide MIDI items for Click, Count, and Guide tracks.
///
/// Returns `(click_items, count_items, guide_items)` that can be placed
/// on the corresponding tracks in the Click + Guide folder.
pub fn generate_guide_items(project: &ReaperProject) -> (Vec<Item>, Vec<Item>, Vec<Item>) {
    let mut click_items = Vec::new();
    let mut count_items = Vec::new();
    let mut guide_items = Vec::new();

    // Get regions (sections)
    let regions: Vec<_> = project
        .markers_regions
        .all
        .iter()
        .filter(|m| m.is_region() && !m.name.is_empty())
        .filter(|m| {
            // Skip structural regions (SONG lane, etc.)
            let upper = m.name.to_uppercase();
            !matches!(
                upper.as_str(),
                "SONGSTART" | "SONGEND" | "=START" | "=END" | "PREROLL" | "POSTROLL"
            )
        })
        .collect();

    if regions.is_empty() {
        return (click_items, count_items, guide_items);
    }

    // Sort regions by position
    let mut regions = regions;
    regions.sort_by(|a, b| a.position.partial_cmp(&b.position).unwrap());

    // Get tempo info
    let (default_bpm, default_num, default_denom) =
        if let Some((bpm, num, denom, _)) = project.properties.tempo {
            (bpm as f64, num as u32, denom as u32)
        } else {
            (120.0, 4, 4)
        };

    let tempo_env = project.tempo_envelope.as_ref();

    // Find the COUNT-IN marker position (if any)
    let count_in_marker = project.markers_regions.all.iter().find(|m| {
        let upper = m.name.to_uppercase();
        matches!(upper.as_str(), "COUNT-IN" | "COUNT IN" | "COUNTIN")
    });

    // Find the overall song extent
    let first_region_start = regions[0].position;
    let last_region_end = regions
        .iter()
        .map(|r| r.end_position.unwrap_or(r.position + 1.0))
        .fold(0.0f64, f64::max);

    let (first_bpm, first_num, first_denom) = tempo_at_position(
        first_region_start,
        tempo_env,
        default_bpm,
        default_num,
        default_denom,
    );
    let first_beat_unit = 4.0 / first_denom as f64;
    let first_measure_seconds = first_beat_unit * first_num as f64 * 60.0 / first_bpm;

    // Count-in starts at the COUNT-IN marker, or one measure before first region
    let song_start = count_in_marker
        .map(|m| m.position)
        .unwrap_or_else(|| (first_region_start - first_measure_seconds).max(0.0));

    // ── Click: single continuous item spanning the entire song ──
    {
        let mut notes = Vec::new();
        let mut pos = song_start;

        while pos < last_region_end - 0.001 {
            let (bpm, num, denom) =
                tempo_at_position(pos, tempo_env, default_bpm, default_num, default_denom);
            let beat_unit = 4.0 / denom as f64;
            let beat_seconds = beat_unit * 60.0 / bpm;
            let measure_seconds = beat_unit * num as f64 * 60.0 / bpm;

            let is_beat_one = {
                let offset = pos - song_start;
                let frac = (offset / measure_seconds).fract();
                !(0.01..=0.99).contains(&frac)
            };
            let (midi_note, velocity) = if is_beat_one {
                (76, 120) // Hi woodblock, accent
            } else {
                (77, 100) // Lo woodblock, normal
            };
            notes.push((pos, midi_note, velocity));
            pos += beat_seconds;
        }

        if !notes.is_empty() {
            click_items.push(make_midi_item(song_start, last_region_end, &notes));
        }
    }

    // ── Count + Guide: per-section items ──
    let mut prev_region_end = 0.0f64;

    for (region_idx, region) in regions.iter().enumerate() {
        let section_start = region.position;
        let section_end = region.end_position.unwrap_or(section_start + 1.0);

        let (bpm, num, denom) = tempo_at_position(
            section_start,
            tempo_env,
            default_bpm,
            default_num,
            default_denom,
        );
        let beat_unit = 4.0 / denom as f64;
        let beat_seconds = beat_unit * 60.0 / bpm;
        let measure_seconds = beat_unit * num as f64 * 60.0 / bpm;

        // Count-in start: use COUNT-IN marker for the first section,
        // one measure before for subsequent sections
        let count_in_start = if region_idx == 0 {
            song_start
        } else {
            (section_start - measure_seconds)
                .max(prev_region_end)
                .max(0.0)
        };

        // Count item: generate count-in pattern
        if count_in_start < section_start - 0.001 {
            let total_count_beats =
                ((section_start - count_in_start) / beat_seconds).round() as u32;
            let total_measures =
                ((section_start - count_in_start) / measure_seconds).round() as u32;

            let mut notes = Vec::new();
            let mut pos = count_in_start;
            for beat_in_count in 0..total_count_beats {
                let measure_idx = beat_in_count / num;
                let beat_in_measure = (beat_in_count % num) + 1;
                let is_last_measure = measure_idx == total_measures.saturating_sub(1);

                // Count-in pattern:
                // - Last measure: all beats (1 2 3 4)
                // - Earlier measures: only beats 1 and 3 (1 x 2 x) for 4/4
                //   or beats 1 and 4 for 6/8, etc.
                let should_count = if is_last_measure {
                    true // All beats in the final measure
                } else {
                    // Sparse: beat 1 and the halfway beat
                    let halfway = (num / 2) + 1;
                    beat_in_measure == 1 || beat_in_measure == halfway
                };

                if should_count {
                    // Map beat number to count number for sparse measures
                    let count_number = if is_last_measure {
                        beat_in_measure
                    } else if beat_in_measure == 1 {
                        measure_idx + 1 // "1" for first sparse measure, "2" for second, etc.
                    } else {
                        measure_idx + 1 + total_measures // Higher number for the "x" beats
                    };
                    let midi_note = 60 + (count_number as u8).min(24);
                    notes.push((pos, midi_note, 100));
                }

                pos += beat_seconds;
            }

            if !notes.is_empty() {
                count_items.push(make_midi_item(count_in_start, section_start, &notes));
            }
        }

        // Guide item: section cue note
        if let Some(midi_note) = section_name_to_midi(&region.name) {
            guide_items.push(make_midi_item(
                count_in_start,
                count_in_start + measure_seconds,
                &[(count_in_start, midi_note, 127)],
            ));
        }

        prev_region_end = section_end;
    }

    (click_items, count_items, guide_items)
}

/// Place generated guide items onto the Click, Count, and Guide tracks
/// in a project's track list.
pub fn apply_guide_items(tracks: &mut [Track], project: &ReaperProject) {
    let (click_items, count_items, guide_items) = generate_guide_items(project);

    for track in tracks.iter_mut() {
        let lower = track.name.to_lowercase();
        match lower.as_str() {
            "click" => track.items.extend(click_items.clone()),
            "count" => track.items.extend(count_items.clone()),
            "guide" => track.items.extend(guide_items.clone()),
            _ => {}
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Get tempo and time signature at a position using the tempo envelope.
fn tempo_at_position(
    position: f64,
    envelope: Option<&TempoTimeEnvelope>,
    default_bpm: f64,
    default_num: u32,
    default_denom: u32,
) -> (f64, u32, u32) {
    let Some(env) = envelope else {
        return (default_bpm, default_num, default_denom);
    };

    let mut bpm = default_bpm;
    let mut num = default_num;
    let mut denom = default_denom;

    for pt in &env.points {
        if pt.position > position {
            break;
        }
        bpm = pt.tempo;
        if let Some(ts) = pt.time_signature_encoded {
            num = (ts & 0xFFFF) as u32;
            denom = (ts >> 16) as u32;
            if num == 0 {
                num = default_num;
            }
            if denom == 0 {
                denom = default_denom;
            }
        }
    }

    (bpm, num, denom)
}

/// Create a MIDI item with note-on/note-off pairs as raw RPP content.
fn make_midi_item(start: f64, end: f64, notes: &[(f64, u8, u8)]) -> Item {
    let duration = end - start;
    let note_length_ticks = 120u32;

    // Sort notes by position
    let mut sorted: Vec<(f64, u8, u8)> = notes.to_vec();
    sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

    // Build MIDI E lines (delta-time encoded)
    let mut midi_lines = Vec::new();
    let mut last_tick = 0u32;

    for &(pos, note, vel) in &sorted {
        let relative = pos - start;
        // Convert seconds to ticks: at the item's local tempo
        // Use a simple linear mapping since items are short
        let tick = if duration > 0.0 {
            ((relative / duration) * (duration * TICKS_PER_QN as f64 * 2.0)).round() as u32
        } else {
            0
        };

        // Note On
        let delta = tick.saturating_sub(last_tick);
        midi_lines.push(format!("E {} 90 {:02x} {:02x}", delta, note, vel));
        last_tick = tick;

        // Note Off
        midi_lines.push(format!("E {} 80 {:02x} 00", note_length_ticks, note));
        last_tick += note_length_ticks;
    }

    let midi_content = midi_lines.join("\n");

    // Build raw RPP item content
    let raw = format!(
        "POSITION {start}\n\
         SNAPOFFS 0\n\
         LENGTH {duration}\n\
         LOOP 0\n\
         ALLTAKES 0\n\
         FADEIN 0 0 0 0 0 0 0\n\
         FADEOUT 0 0 0 0 0 0 0\n\
         MUTE 0 0\n\
         SEL 0\n\
         <SOURCE MIDI\n\
         HASDATA 1 {TICKS_PER_QN} QN\n\
         {midi_content}\n\
         E {last_tick} b0 7b 00\n\
         >\n"
    );

    Item {
        position: start,
        length: duration,
        raw_content: raw,
        ..Item::default()
    }
}

/// Map a section name to a MIDI note number for the guide cue.
///
/// Uses the same mapping as keyflow's section cue system.
fn section_name_to_midi(name: &str) -> Option<u8> {
    let upper = name.split_whitespace().next().unwrap_or("").to_uppercase();
    // Remove trailing numbers: "VS 1" → "VS", "CH 2" → "CH"
    match upper.as_str() {
        "INTRO" => Some(36),                                       // C2
        "VS" | "VERSE" => Some(38),                                // D2
        "PRE" | "PRECHORUS" | "PRE-CHORUS" | "PRE-CH" => Some(40), // E2
        "CH" | "CHORUS" => Some(41),                               // F2
        "BR" | "BRIDGE" => Some(43),                               // G2
        "SOLO" => Some(45),                                        // A2
        "OUTRO" => Some(47),                                       // B2
        "BREAK" | "BREAKDOWN" => Some(48),                         // C3
        "INTERLUDE" => Some(50),                                   // D3
        "INSTRUMENTAL" => Some(52),                                // E3
        "TAG" | "VAMP" => Some(53),                                // F3
        "HITS" => Some(55),                                        // G3
        _ => Some(60),                                             // C4 — generic section
    }
}

//! Fixed lanes and comping on the live REAPER backend.
//!
//! Two halves. The lanes themselves — count, play mask, names, display —
//! have SDK accessors (`I_FREEMODE`, `I_NUMFIXEDLANES`, `C_LANEPLAYS:N`,
//! `C_LANESETTINGS`, `C_LANESCOLLAPSED`, `P_LANENAME:n`; SDK header
//! `reaper_plugin_functions.h` around the `GetSetMediaTrackInfo` table)
//! and are read and written through them. The comping state has none:
//! `LANEREC`, `ITEMLANES` and `LINKEDLANE` exist only in the track's state
//! chunk, so [`comping_from_chunk`] and [`patch_chunk_comping`] are a
//! line-level codec over `GetTrackStateChunk` / `SetTrackStateChunk`. Both
//! are pure text functions so they are unit-tested without REAPER.

use std::ffi::CString;
use std::os::raw::{c_char, c_void};

use daw_proto::track::{CompArea, LaneComping, LaneDisplay};
use daw_proto::{DawError, DawResult, Duration, PositionInSeconds};
use dawfile_reaper::rpp_tree::tokenize;
use dawfile_reaper::types::track::{
    FixedLaneState, comping_from_lines, comping_lines, lane_settings,
};
use reaper_medium::{ChunkCacheHint, MediaTrack, TrackAttributeKey};

/// `I_FREEMODE` value meaning "fixed lanes enabled".
const FREEMODE_FIXED_LANES: f64 = 2.0;

fn medium() -> &'static reaper_medium::Reaper {
    reaper_high::Reaper::get().medium_reaper()
}

/// Whether the track is in fixed-lanes mode.
pub fn has_fixed_lanes(track: MediaTrack) -> bool {
    // SAFETY: called on the main thread with a track the caller resolved.
    let mode = unsafe {
        medium().get_media_track_info_value(track, TrackAttributeKey::custom("I_FREEMODE"))
    };
    mode == FREEMODE_FIXED_LANES
}

/// Read one of REAPER's `char*` track attributes (`C_…`).
fn char_attr(track: MediaTrack, key: &str) -> Option<i8> {
    // SAFETY: a `C_` key returns a pointer to a char owned by REAPER, valid
    // until the next API call; it is read immediately.
    let ptr = unsafe {
        medium().get_set_media_track_info(
            track,
            TrackAttributeKey::custom(key),
            std::ptr::null_mut(),
        )
    };
    if ptr.is_null() {
        None
    } else {
        Some(unsafe { *(ptr as *const c_char) } as i8)
    }
}

/// Write one of REAPER's `char*` track attributes.
fn set_char_attr(track: MediaTrack, key: &str, value: i8) {
    let mut value = value as c_char;
    // SAFETY: REAPER reads the char through the pointer during the call only.
    unsafe {
        medium().get_set_media_track_info(
            track,
            TrackAttributeKey::custom(key),
            &mut value as *mut c_char as *mut c_void,
        );
    }
}

fn lane_name(track: MediaTrack, lane: u32) -> Option<String> {
    let key = CString::new(format!("P_LANENAME:{lane}")).ok()?;
    let mut buf = vec![0u8; 1024];
    // SAFETY: the buffer outlives the call and the key is NUL-terminated.
    let ok = unsafe {
        medium().low().GetSetMediaTrackInfo_String(
            track.as_ptr(),
            key.as_ptr(),
            buf.as_mut_ptr() as *mut c_char,
            false,
        )
    };
    ok.then(|| crate::safe_wrappers::buffer::string_from_buffer(&buf))
}

fn set_lane_name_raw(track: MediaTrack, lane: u32, name: &str) -> DawResult<()> {
    let key = CString::new(format!("P_LANENAME:{lane}"))
        .map_err(|e| DawError::operation_failed(format!("bad lane key: {e}")))?;
    let value = CString::new(name)
        .map_err(|e| DawError::operation_failed(format!("lane name contains a NUL: {e}")))?;
    let mut buf = value.into_bytes_with_nul();
    // SAFETY: both strings are NUL-terminated and outlive the call.
    let ok = unsafe {
        medium().low().GetSetMediaTrackInfo_String(
            track.as_ptr(),
            key.as_ptr(),
            buf.as_mut_ptr() as *mut c_char,
            true,
        )
    };
    if ok {
        Ok(())
    } else {
        Err(DawError::operation_failed(format!(
            "set P_LANENAME:{lane} failed"
        )))
    }
}

/// The track's fixed lanes, from the live API. All zeros on a track
/// that is not in fixed-lanes mode.
pub fn read_lanes(track: MediaTrack) -> FixedLaneState {
    if !has_fixed_lanes(track) {
        return FixedLaneState::default();
    }
    // SAFETY: main thread, resolved track.
    let lane_count = unsafe {
        medium().get_media_track_info_value(track, TrackAttributeKey::custom("I_NUMFIXEDLANES"))
    }
    .max(0.0) as u32;
    let mut lane_play_mask = 0u64;
    let mut lane_names = Vec::with_capacity(lane_count as usize);
    for lane in 0..lane_count.min(64) {
        // C_LANEPLAYS:N — 0 silent, 1 plays exclusively, 2 plays in layers.
        if char_attr(track, &format!("C_LANEPLAYS:{lane}")).is_some_and(|v| v > 0) {
            lane_play_mask |= 1 << lane;
        }
    }
    for lane in 0..lane_count {
        lane_names.push(lane_name(track, lane).unwrap_or_else(|| (lane + 1).to_string()));
    }
    let settings = char_attr(track, "C_LANESETTINGS").unwrap_or(0);
    // `C_LANESCOLLAPSED` 1 is "lanes collapsed" — the one-lane end of
    // REAPER's lane-button cycle, and the live-API counterpart of the
    // `FIXEDLANES` field the project-file loaders map to `One`. The
    // correspondence is read off REAPER's own output, not the SDK header,
    // which documents neither field against the other; if they ever prove
    // to be two settings rather than one, this is the line that is wrong.
    let collapsed = char_attr(track, "C_LANESCOLLAPSED").unwrap_or(0);
    let lane_display = if collapsed == 1 {
        LaneDisplay::One
    } else if settings & big_lanes_bit() != 0 {
        LaneDisplay::Big
    } else {
        LaneDisplay::Small
    };
    FixedLaneState {
        lane_count,
        lane_play_mask,
        lane_names,
        lane_display,
    }
}

/// The `C_LANESETTINGS` bits, as `char*` attributes carry them. The
/// values are `dawfile-reaper`'s, so the live API and the project file
/// read the same bitfield the same way.
fn big_lanes_bit() -> i8 {
    lane_settings::BIG_LANES as i8
}

fn auto_remove_empty_bit() -> i8 {
    lane_settings::AUTO_REMOVE_EMPTY as i8
}

/// Set the lane count, switching fixed-lanes mode on or off with it.
///
/// REAPER's default lane settings auto-remove empty lanes at the bottom
/// (`C_LANESETTINGS & 1`), so asking for three empty take lanes otherwise
/// leaves one: the count is a *request* until something is on the lane.
/// A caller of this method is stating the layout it wants, so the bit is
/// cleared first and the count then sticks.
pub fn write_lane_count(track: MediaTrack, count: u32) -> DawResult<()> {
    let m = medium();
    let mode = if count > 0 { FREEMODE_FIXED_LANES } else { 0.0 };
    // SAFETY: main thread, resolved track.
    unsafe {
        m.set_media_track_info_value(track, TrackAttributeKey::custom("I_FREEMODE"), mode)
            .map_err(|e| DawError::operation_failed(format!("set I_FREEMODE failed: {e}")))?;
        if count > 0 {
            let settings = char_attr(track, "C_LANESETTINGS").unwrap_or(0);
            if settings & auto_remove_empty_bit() != 0 {
                set_char_attr(track, "C_LANESETTINGS", settings & !auto_remove_empty_bit());
            }
            m.set_media_track_info_value(
                track,
                TrackAttributeKey::custom("I_NUMFIXEDLANES"),
                f64::from(count),
            )
            .map_err(|e| DawError::operation_failed(format!("set I_NUMFIXEDLANES failed: {e}")))?;
        }
    }
    if count == 0 {
        // Lanes off takes the comping with them: REAPER leaves LANEREC and
        // the LINKEDLANE areas in the chunk, and re-enabling lanes later
        // would otherwise come back holding a comp of lanes long gone.
        let chunk = track_chunk(track)?;
        set_track_chunk(
            track,
            &patch_chunk_comping(&chunk, &LaneComping::default(), 0),
        )?;
    }
    m.update_timeline();
    let got = read_lanes(track).lane_count;
    if got == count {
        Ok(())
    } else {
        Err(DawError::operation_failed(format!(
            "asked REAPER for {count} fixed lanes, it kept {got}"
        )))
    }
}

/// Set which lanes play. Playing lanes are set first (in layers, `2`),
/// then the rest cleared, so REAPER never sees a moment with no lane
/// playing and re-enables lane 0 behind our back.
pub fn write_lane_play_mask(track: MediaTrack, mask: u64) -> DawResult<()> {
    let count = read_lanes(track).lane_count.min(64);
    if count == 0 {
        return Err(DawError::operation_failed("track has no fixed lanes"));
    }
    if mask == 0 {
        set_char_attr(track, "C_ALLLANESPLAY", 0);
    } else {
        for lane in (0..count).filter(|l| mask & (1 << l) != 0) {
            set_char_attr(track, &format!("C_LANEPLAYS:{lane}"), 2);
        }
        for lane in (0..count).filter(|l| mask & (1 << l) == 0) {
            set_char_attr(track, &format!("C_LANEPLAYS:{lane}"), 0);
        }
    }
    medium().update_timeline();
    Ok(())
}

/// Name a lane the track has.
pub fn write_lane_name(track: MediaTrack, lane: u32, name: &str) -> DawResult<()> {
    let count = read_lanes(track).lane_count;
    if lane >= count {
        return Err(DawError::out_of_range(lane, count, "fixed lane"));
    }
    set_lane_name_raw(track, lane, name)?;
    medium().update_timeline();
    Ok(())
}

/// The track's state chunk. Tried at 1 MiB, then 16 MiB — a chunk holds
/// every item and FX state, and REAPER fails rather than truncates.
pub fn track_chunk(track: MediaTrack) -> DawResult<String> {
    for size in [1 << 20, 16 << 20] {
        // SAFETY: main thread, resolved track.
        if let Ok(chunk) =
            unsafe { medium().get_track_state_chunk(track, size, ChunkCacheHint::NormalMode) }
        {
            return Ok(chunk.into_string());
        }
    }
    Err(DawError::operation_failed("GetTrackStateChunk failed"))
}

pub fn set_track_chunk(track: MediaTrack, chunk: &str) -> DawResult<()> {
    // SAFETY: main thread, resolved track, chunk is a well-formed <TRACK block.
    unsafe { medium().set_track_state_chunk(track, chunk, ChunkCacheHint::NormalMode) }
        .map_err(|e| DawError::operation_failed(format!("SetTrackStateChunk failed: {e}")))
}

// ── the chunk codec ────────────────────────────────────────────────────

/// Depth of each line in a chunk: the `<TRACK` header is 0, its own
/// lines 1, an item's lines 2, and so on.
fn depth_one_lines(chunk: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut depth = 0i32;
    chunk.lines().enumerate().filter_map(move |(i, line)| {
        let t = line.trim_start();
        if t.starts_with('<') {
            depth += 1;
            None
        } else if t == ">" {
            depth -= 1;
            None
        } else if depth == 1 {
            Some((i, t))
        } else {
            None
        }
    })
}

/// The ext-state a track chunk carries (`P_EXT`), by key: written as
/// `EXT key value` lines of the track, or as the lines of an `<EXT`
/// block in it.
#[must_use]
pub fn ext_from_chunk(chunk: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut depth = 0_i32;
    let mut in_ext = false;
    for line in chunk.lines() {
        let t = line.trim();
        if let Some(block) = t.strip_prefix('<') {
            depth = depth.saturating_add(1);
            in_ext = depth == 2 && block.trim() == "EXT";
            continue;
        }
        if t == ">" {
            depth = depth.saturating_sub(1);
            in_ext = false;
            continue;
        }
        let pair = if depth == 1 {
            t.strip_prefix("EXT ")
        } else if in_ext {
            Some(t)
        } else {
            None
        };
        if let Some((key, value)) = pair.and_then(|rest| rest.split_once(' ')) {
            out.push((key.to_owned(), value.trim().to_owned()));
        }
    }
    out.sort();
    out
}

const COMPING_KEYS: [&str; 3] = ["LANEREC", "ITEMLANES", "LINKEDLANE"];

fn key_of(line: &str) -> &str {
    line.split_whitespace().next().unwrap_or_default()
}

/// The comping state in a track chunk: `LANEREC` and every `LINKEDLANE`.
pub fn comping_from_chunk(chunk: &str) -> LaneComping {
    let lines: Vec<Vec<String>> = depth_one_lines(chunk)
        .filter(|(_, line)| COMPING_KEYS.contains(&key_of(line)))
        .map(|(_, line)| tokenize(line).into_iter().map(|t| t.token).collect())
        .collect();
    comping_from_lines(lines.iter().map(Vec::as_slice))
}

/// The chunk with its comping lines replaced by `comping`'s.
///
/// The old lines are dropped and the new ones go where the first old one
/// was — or before the track's first nested block, or before its closing
/// `>` — so everything else in the chunk stays exactly where REAPER put it.
pub fn patch_chunk_comping(chunk: &str, comping: &LaneComping, lane_count: u32) -> String {
    let old: std::collections::HashSet<usize> = depth_one_lines(chunk)
        .filter(|(_, line)| COMPING_KEYS.contains(&key_of(line)))
        .map(|(i, _)| i)
        .collect();
    let lines: Vec<&str> = chunk.lines().collect();
    let insert_at = old.iter().min().copied().unwrap_or_else(|| {
        // Before the first nested block at depth 1, else before the
        // closing `>` of the track.
        let mut depth = 0i32;
        let mut at = lines.len().saturating_sub(1);
        for (i, line) in lines.iter().enumerate() {
            let t = line.trim_start();
            if t.starts_with('<') {
                if depth == 1 {
                    at = i;
                    break;
                }
                depth += 1;
            } else if t == ">" {
                depth -= 1;
            }
        }
        at
    });
    let indent = lines
        .get(insert_at)
        .map(|l| &l[..l.len() - l.trim_start().len()])
        .filter(|s| !s.is_empty())
        .unwrap_or("  ");
    let mut out = String::with_capacity(chunk.len() + 64);
    for (i, line) in lines.iter().enumerate() {
        if i == insert_at {
            for new in comping_lines(comping, lane_count) {
                out.push_str(indent);
                out.push_str(&new.join(" "));
                out.push('\n');
            }
        }
        if old.contains(&i) {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHUNK: &str = "<TRACK {61541BCB-476B-E905-C722-AF3AA7DC67A6}
  NAME In
  FREEMODE 2
  FIXEDLANES 9 0 0 0 0
  LANESOLO 1 0 0 0 0 0 0 0
  LANEREC -1 0 1
  LANENAME \"Custom Lane Name\" C1 1 2 3
  MAINSEND 1 0
  ITEMLANES 5
  LINKEDLANE 2 5.20533333333333 4 0 -1 0.01 0.01
  LINKEDLANE 2 5.20533333333333 4 1 -1 0.01 0.01
  <ITEM
    POSITION 2
    LENGTH 3.20533333333333
    <SOURCE WAVE
      FILE take.wav
    >
  >
>
";

    fn area(comp_lane: u32, source_lane: u32, start: f64, end: f64) -> CompArea {
        CompArea {
            start: PositionInSeconds::from_seconds(start),
            end: PositionInSeconds::from_seconds(end),
            source_lane,
            comp_lane,
            fade_in: Duration::from_seconds(0.01),
            fade_out: Duration::from_seconds(0.01),
        }
    }

    #[test]
    fn ext_state_reads_from_lines_and_from_a_block() {
        let chunk = "<TRACK\nNAME Kick\nEXT FTS:kind piece\n<EXT\nFTS:group Drums/Drum Kit/Kick\n>\n<ITEM\nEXT not:ours x\n>\n>";
        assert_eq!(
            ext_from_chunk(chunk),
            [
                ("FTS:group".to_owned(), "Drums/Drum Kit/Kick".to_owned()),
                ("FTS:kind".to_owned(), "piece".to_owned()),
            ]
        );
    }

    #[test]
    fn the_fixture_chunk_reads_as_reaper_wrote_it() {
        let comping = comping_from_chunk(CHUNK);
        assert_eq!(
            comping,
            LaneComping {
                record_lane: None,
                comp_lane: Some(0),
                last_comp_lane: Some(1),
                areas: vec![
                    area(0, 4, 2.0, 5.20533333333333),
                    area(1, 4, 2.0, 5.20533333333333)
                ],
            }
        );
    }

    #[test]
    fn nested_lines_never_read_as_the_tracks() {
        // An item's own lines sit at depth 2 and must not be mistaken for
        // track lines, whatever their key.
        let chunk = "<TRACK\n  <ITEM\n    LANEREC 3 3 3\n  >\n>\n";
        assert_eq!(comping_from_chunk(chunk), LaneComping::default());
    }

    #[test]
    fn patching_replaces_the_comping_lines_in_place() {
        let comping = LaneComping {
            record_lane: Some(2),
            comp_lane: Some(1),
            last_comp_lane: Some(0),
            areas: vec![area(1, 3, 2.0, 3.5)],
        };
        let patched = patch_chunk_comping(CHUNK, &comping, 5);
        assert_eq!(comping_from_chunk(&patched), comping);
        assert!(
            patched.contains(
                "  LANEREC 2 1 0\n  ITEMLANES 5\n  LINKEDLANE 2 3.5 3 1 -1 0.01 0.01\n  LANENAME"
            ),
            "{patched}"
        );
        // Everything else is untouched, including the item's lines.
        assert!(patched.contains("  LANESOLO 1 0 0 0 0 0 0 0\n"));
        assert!(patched.contains("    LENGTH 3.20533333333333\n"));
        assert_eq!(patched.lines().count(), CHUNK.lines().count() - 1);
    }

    #[test]
    fn patching_a_chunk_without_comping_lines_puts_them_before_the_items() {
        let chunk = "<TRACK\n  NAME Vox\n  FREEMODE 2\n  <ITEM\n    POSITION 0\n  >\n>\n";
        let comping = LaneComping {
            comp_lane: Some(2),
            ..Default::default()
        };
        let patched = patch_chunk_comping(chunk, &comping, 3);
        assert_eq!(
            patched,
            "<TRACK\n  NAME Vox\n  FREEMODE 2\n  LANEREC -1 2 -1\n  ITEMLANES 3\n  <ITEM\n    POSITION 0\n  >\n>\n"
        );
        assert_eq!(comping_from_chunk(&patched), comping);
    }

    #[test]
    fn patching_an_empty_track_puts_them_before_the_close() {
        let chunk = "<TRACK\n  NAME Vox\n>\n";
        let patched = patch_chunk_comping(chunk, &LaneComping::default(), 2);
        assert_eq!(patched, "<TRACK\n  NAME Vox\n  ITEMLANES 2\n>\n");
    }

    #[test]
    fn clearing_comping_drops_lanerec_and_the_areas() {
        let patched = patch_chunk_comping(CHUNK, &LaneComping::default(), 5);
        assert!(!patched.contains("LANEREC"));
        assert!(!patched.contains("LINKEDLANE"));
        assert!(patched.contains("  ITEMLANES 5\n"));
    }
}

//! Safe wrappers around `reaper-low` envelope APIs.
//!
//! reaper-medium / reaper-high don't expose the envelope CRUD surface we
//! need (point insertion, point reads with shape + tension, evaluation,
//! state-chunk getters). This module wraps the C calls in Rust types
//! and assumes the caller is on the REAPER main thread (use
//! [`crate::main_thread::query`] / [`crate::main_thread::run`] from
//! service code).
//!
//! All functions take `&Reaper` (low-level) so callers don't have to
//! re-deref the pointer chain. Pointers (`MediaTrack`, `TrackEnvelope`)
//! are passed through unchanged — REAPER guarantees their stability for
//! the lifetime of the project. Each function null-checks before
//! dereffing, so passing dangling/null pointers is defined behavior
//! (returns `None` / `false` / `0`).
#![allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]

use reaper_low::Reaper;
use reaper_low::raw::{MediaTrack, TrackEnvelope};
use std::ffi::{CStr, CString};

/// Number of envelopes attached to a track. `0` if `track` is null.
pub fn count_track_envelopes(low: &Reaper, track: *mut MediaTrack) -> u32 {
    if track.is_null() {
        return 0;
    }
    unsafe { low.CountTrackEnvelopes(track).max(0) as u32 }
}

/// Get the Nth track envelope by index. Returns null if out of range.
pub fn get_track_envelope(low: &Reaper, track: *mut MediaTrack, index: u32) -> *mut TrackEnvelope {
    if track.is_null() {
        return std::ptr::null_mut();
    }
    unsafe { low.GetTrackEnvelope(track, index as i32) }
}

/// Resolve a track envelope by display name (e.g. "Volume", "Pan",
/// "Width"). Returns null if no match.
pub fn get_track_envelope_by_name(
    low: &Reaper,
    track: *mut MediaTrack,
    name: &str,
) -> *mut TrackEnvelope {
    if track.is_null() {
        return std::ptr::null_mut();
    }
    let Ok(c) = CString::new(name) else {
        return std::ptr::null_mut();
    };
    unsafe { low.GetTrackEnvelopeByName(track, c.as_ptr()) }
}

/// Resolve a track envelope by chunk-name tag (e.g. `<VOLENV2`,
/// `<PANENV2`). Used when distinguishing pre-/post-FX envelopes that
/// share a display name.
pub fn get_track_envelope_by_chunk_name(
    low: &Reaper,
    track: *mut MediaTrack,
    chunk_name: &str,
) -> *mut TrackEnvelope {
    if track.is_null() {
        return std::ptr::null_mut();
    }
    let Ok(c) = CString::new(chunk_name) else {
        return std::ptr::null_mut();
    };
    unsafe { low.GetTrackEnvelopeByChunkName(track, c.as_ptr()) }
}

/// Get the human-readable envelope name. Returns `None` on failure.
pub fn get_envelope_name(low: &Reaper, envelope: *mut TrackEnvelope) -> Option<String> {
    if envelope.is_null() {
        return None;
    }
    let mut buf = vec![0i8; 256];
    let ok = unsafe { low.GetEnvelopeName(envelope, buf.as_mut_ptr() as *mut _, buf.len() as i32) };
    if !ok {
        return None;
    }
    let cstr = unsafe { CStr::from_ptr(buf.as_ptr() as *const _) };
    cstr.to_str().ok().map(|s| s.to_string())
}

/// Number of points in an envelope. `0` for null envelope.
pub fn count_envelope_points(low: &Reaper, envelope: *mut TrackEnvelope) -> u32 {
    if envelope.is_null() {
        return 0;
    }
    unsafe { low.CountEnvelopePoints(envelope).max(0) as u32 }
}

/// Single envelope point read.
#[derive(Clone, Debug, Default)]
pub struct PointSample {
    pub time: f64,
    pub value: f64,
    /// Raw REAPER shape index (0 linear, 1 square, 2 slow start/end,
    /// 3 fast start, 4 fast end, 5 bezier).
    pub shape: i32,
    pub tension: f64,
    pub selected: bool,
}

/// Read a single point. Returns `None` if `index` is out of range.
pub fn get_envelope_point(
    low: &Reaper,
    envelope: *mut TrackEnvelope,
    index: u32,
) -> Option<PointSample> {
    if envelope.is_null() {
        return None;
    }
    let mut time = 0.0f64;
    let mut value = 0.0f64;
    let mut shape = 0i32;
    let mut tension = 0.0f64;
    let mut selected = false;
    let ok = unsafe {
        low.GetEnvelopePoint(
            envelope,
            index as i32,
            &mut time,
            &mut value,
            &mut shape,
            &mut tension,
            &mut selected,
        )
    };
    if !ok {
        return None;
    }
    Some(PointSample {
        time,
        value,
        shape,
        tension,
        selected,
    })
}

/// Insert a new point. Returns `true` on success.
///
/// `sort` should generally be `true` so the envelope's points stay in
/// time order; pass `false` only when batching multiple inserts and
/// calling [`sort_envelope_points`] yourself afterwards.
pub fn insert_envelope_point(
    low: &Reaper,
    envelope: *mut TrackEnvelope,
    time: f64,
    value: f64,
    shape: i32,
    tension: f64,
    selected: bool,
    sort: bool,
) -> bool {
    if envelope.is_null() {
        return false;
    }
    let mut sort_in = !sort;
    unsafe {
        low.InsertEnvelopePoint(
            envelope,
            time,
            value,
            shape,
            tension,
            selected,
            &mut sort_in,
        )
    }
}

/// Update an existing point in place. Pass `None` for any field you
/// don't want to change. Returns `true` on success.
pub fn set_envelope_point(
    low: &Reaper,
    envelope: *mut TrackEnvelope,
    index: u32,
    time: Option<f64>,
    value: Option<f64>,
    shape: Option<i32>,
    tension: Option<f64>,
    selected: Option<bool>,
    sort: bool,
) -> bool {
    if envelope.is_null() {
        return false;
    }
    let mut t = time.unwrap_or(0.0);
    let mut v = value.unwrap_or(0.0);
    let mut s = shape.unwrap_or(0);
    let mut ten = tension.unwrap_or(0.0);
    let mut sel = selected.unwrap_or(false);
    let mut sort_in = !sort;
    unsafe {
        low.SetEnvelopePoint(
            envelope,
            index as i32,
            if time.is_some() {
                &mut t
            } else {
                std::ptr::null_mut()
            },
            if value.is_some() {
                &mut v
            } else {
                std::ptr::null_mut()
            },
            if shape.is_some() {
                &mut s
            } else {
                std::ptr::null_mut()
            },
            if tension.is_some() {
                &mut ten
            } else {
                std::ptr::null_mut()
            },
            if selected.is_some() {
                &mut sel
            } else {
                std::ptr::null_mut()
            },
            &mut sort_in,
        )
    }
}

/// Delete a single point by index. Returns `true` on success.
pub fn delete_envelope_point(low: &Reaper, envelope: *mut TrackEnvelope, index: u32) -> bool {
    if envelope.is_null() {
        return false;
    }
    unsafe { low.DeleteEnvelopePointEx(envelope, -1, index as i32) }
}

/// Delete every point with `start <= time <= end`. Returns `true` if
/// at least one point was removed.
pub fn delete_envelope_points_in_range(
    low: &Reaper,
    envelope: *mut TrackEnvelope,
    start: f64,
    end: f64,
) -> bool {
    if envelope.is_null() {
        return false;
    }
    unsafe { low.DeleteEnvelopePointRange(envelope, start, end) }
}

/// Force-sort an envelope's points by time. Use after batched
/// `insert_envelope_point(.., sort=false)` calls.
pub fn sort_envelope_points(low: &Reaper, envelope: *mut TrackEnvelope) -> bool {
    if envelope.is_null() {
        return false;
    }
    unsafe { low.Envelope_SortPoints(envelope) }
}

/// Evaluate the envelope at a specific time. Returns `(value, dvds,
/// ddvds, dddvds)` — value plus derivatives. Returns `None` if the
/// envelope is null.
pub fn evaluate_envelope(
    low: &Reaper,
    envelope: *mut TrackEnvelope,
    time: f64,
    samplerate: f64,
    samples_requested: i32,
) -> Option<(f64, f64, f64, f64)> {
    if envelope.is_null() {
        return None;
    }
    let mut value = 0.0f64;
    let mut dvds = 0.0f64;
    let mut ddvds = 0.0f64;
    let mut dddvds = 0.0f64;
    let _shape_changed = unsafe {
        low.Envelope_Evaluate(
            envelope,
            time,
            samplerate,
            samples_requested,
            &mut value,
            &mut dvds,
            &mut ddvds,
            &mut dddvds,
        )
    };
    Some((value, dvds, ddvds, dddvds))
}

// ── Automation items ────────────────────────────────────────────────
//
// REAPER addresses an automation item by `(envelope, index)` and reads
// or writes every field through one `GetSetAutomationItemInfo` call with
// a descriptor string. The `_Ex` point functions take the same index —
// `-1` means the envelope's own points, `>= 0` means the item's — which
// is why an item's curve is a different read from the envelope's.

/// Number of automation items on an envelope. `0` if `envelope` is null.
pub fn count_automation_items(low: &Reaper, envelope: *mut TrackEnvelope) -> u32 {
    if envelope.is_null() {
        return 0;
    }
    unsafe { low.CountAutomationItems(envelope).max(0) as u32 }
}

/// Read one numeric field of an automation item.
///
/// Descriptors are REAPER's: `D_POS`, `D_LENGTH`, `D_STARTOFFS`,
/// `D_PLAYRATE`, `D_BASELINE`, `D_AMPLITUDE`, `D_LOOPSRC`, `D_UISEL`,
/// `P_POOL_ID`.
pub fn get_automation_item_info(
    low: &Reaper,
    envelope: *mut TrackEnvelope,
    index: u32,
    desc: &str,
) -> f64 {
    if envelope.is_null() {
        return 0.0;
    }
    let Ok(desc) = CString::new(desc) else {
        return 0.0;
    };
    unsafe { low.GetSetAutomationItemInfo(envelope, index as i32, desc.as_ptr(), 0.0, false) }
}

/// Write one numeric field of an automation item.
pub fn set_automation_item_info(
    low: &Reaper,
    envelope: *mut TrackEnvelope,
    index: u32,
    desc: &str,
    value: f64,
) {
    if envelope.is_null() {
        return;
    }
    let Ok(desc) = CString::new(desc) else {
        return;
    };
    unsafe {
        low.GetSetAutomationItemInfo(envelope, index as i32, desc.as_ptr(), value, true);
    }
}

/// The pooled source's name (`P_POOL_NAME`).
pub fn get_automation_item_name(
    low: &Reaper,
    envelope: *mut TrackEnvelope,
    index: u32,
) -> Option<String> {
    if envelope.is_null() {
        return None;
    }
    let desc = CString::new("P_POOL_NAME").ok()?;
    let mut buf = vec![0i8; 512];
    let ok = unsafe {
        low.GetSetAutomationItemInfo_String(
            envelope,
            index as i32,
            desc.as_ptr(),
            buf.as_mut_ptr(),
            false,
        )
    };
    if !ok {
        return None;
    }
    let name = unsafe { CStr::from_ptr(buf.as_ptr()) }
        .to_string_lossy()
        .into_owned();
    Some(name)
}

/// Insert an automation item over a range. `pool_id` of `-1` makes a
/// fresh pool. Returns the new item's index, or `None` on refusal.
pub fn insert_automation_item(
    low: &Reaper,
    envelope: *mut TrackEnvelope,
    pool_id: i32,
    position: f64,
    length: f64,
) -> Option<u32> {
    if envelope.is_null() {
        return None;
    }
    let index = unsafe { low.InsertAutomationItem(envelope, pool_id, position, length) };
    (index >= 0).then_some(index as u32)
}

/// Points *inside* an automation item — `GetEnvelopePointEx` with the
/// item's index, where `-1` would read the envelope's own points.
pub fn get_automation_item_points(
    low: &Reaper,
    envelope: *mut TrackEnvelope,
    index: u32,
) -> Vec<PointSample> {
    if envelope.is_null() {
        return Vec::new();
    }
    let count = unsafe { low.CountEnvelopePointsEx(envelope, index as i32).max(0) } as u32;
    let mut out = Vec::with_capacity(count as usize);
    for i in 0..count {
        let mut time = 0.0f64;
        let mut value = 0.0f64;
        let mut shape = 0i32;
        let mut tension = 0.0f64;
        let mut selected = false;
        let ok = unsafe {
            low.GetEnvelopePointEx(
                envelope,
                index as i32,
                i as i32,
                &mut time,
                &mut value,
                &mut shape,
                &mut tension,
                &mut selected,
            )
        };
        if ok {
            out.push(PointSample {
                time,
                value,
                shape,
                tension,
                selected,
            });
        }
    }
    out
}

/// One field of `GetEnvelopeInfo_Value` — e.g. `I_TCPH` for the lane's
/// height in pixels, `I_TCPY` for its offset.
pub fn get_envelope_info_value(low: &Reaper, envelope: *mut TrackEnvelope, desc: &str) -> f64 {
    if envelope.is_null() {
        return 0.0;
    }
    let Ok(desc) = CString::new(desc) else {
        return 0.0;
    };
    unsafe { low.GetEnvelopeInfo_Value(envelope, desc.as_ptr()) }
}

/// The envelope's state chunk.
///
/// The lane facts live here and nowhere else in the API: `VIS vis lane
/// unknown` carries visibility and the in-own-lane flag, `LANEHEIGHT h
/// unknown` the height. `GetEnvelopeInfo_Value`'s `I_TCPH` reports the
/// *laid-out* height, which is 0 for a hidden lane and cannot be
/// written — so reads can use either but writes must go through here.
pub fn get_envelope_state_chunk(low: &Reaper, envelope: *mut TrackEnvelope) -> Option<String> {
    if envelope.is_null() {
        return None;
    }
    let mut buf = vec![0i8; 64 * 1024];
    let ok =
        unsafe { low.GetEnvelopeStateChunk(envelope, buf.as_mut_ptr(), buf.len() as i32, false) };
    if !ok {
        return None;
    }
    Some(
        unsafe { CStr::from_ptr(buf.as_ptr()) }
            .to_string_lossy()
            .into_owned(),
    )
}

/// Write the envelope's state chunk back.
pub fn set_envelope_state_chunk(low: &Reaper, envelope: *mut TrackEnvelope, chunk: &str) -> bool {
    if envelope.is_null() {
        return false;
    }
    let Ok(chunk) = CString::new(chunk) else {
        return false;
    };
    unsafe { low.SetEnvelopeStateChunk(envelope, chunk.as_ptr(), false) }
}

/// The lane facts, parsed out of the state chunk.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LaneState {
    pub visible: bool,
    /// `VIS`'s second field: the envelope has a lane of its own.
    pub in_own_lane: bool,
    /// `LANEHEIGHT`'s first field, in pixels.
    pub height: u32,
    /// `ARM`'s only field: armed for automation recording.
    pub armed: bool,
    /// `ACT`'s second field — REAPER's automation mode for this
    /// envelope, or `-1` meaning "follow the track's".
    pub automation_mode: i32,
    /// `ACT`'s first field: whether the envelope is active at all.
    /// Read and preserved rather than exposed — an inactive envelope
    /// is a state the facade does not model yet, and a rewrite must
    /// not silently activate one.
    pub active: bool,
}

impl Default for LaneState {
    /// An envelope with no `ACT` line is active and follows the track's
    /// automation mode — `false`/`0` would read as "inactive, trim/read"
    /// and a rewrite would then deactivate it.
    fn default() -> Self {
        Self {
            visible: false,
            in_own_lane: false,
            height: 0,
            armed: false,
            automation_mode: -1,
            active: true,
        }
    }
}

/// Read `VIS`, `LANEHEIGHT`, `ARM` and `ACT` out of a chunk.
///
/// Line-wise rather than by regex: the chunk is line-oriented and both
/// keys are the first word of their line, so this cannot be confused by
/// a nested `<PARMENV>` block carrying its own.
pub fn parse_lane_state(chunk: &str) -> LaneState {
    let mut state = LaneState::default();
    // Only the outermost block's keys — a nested block is indented and
    // belongs to a parameter envelope, not this one.
    for line in chunk.lines() {
        let mut words = line.split_whitespace();
        match words.next() {
            Some("VIS") => {
                state.visible = words.next().map(|v| v != "0").unwrap_or(false);
                state.in_own_lane = words.next().map(|v| v != "0").unwrap_or(false);
            }
            Some("LANEHEIGHT") => {
                state.height = words.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            }
            Some("ARM") => {
                state.armed = words.next().map(|v| v != "0").unwrap_or(false);
            }
            Some("ACT") => {
                state.active = words.next().map(|v| v != "0").unwrap_or(true);
                state.automation_mode = words.next().and_then(|v| v.parse().ok()).unwrap_or(-1);
            }
            _ => {}
        }
    }
    state
}

/// Rewrite `VIS` and `LANEHEIGHT` in a chunk, leaving every other line
/// exactly as it was.
///
/// Surgical for the same reason `fts_themer::thresholds` is: the chunk
/// is REAPER's own serialisation of everything about this envelope, and
/// a rewrite that regenerates it would drop what this code does not
/// model.
pub fn splice_lane_state(chunk: &str, lane: LaneState) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut wrote_height = false;
    for line in chunk.lines() {
        let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
        let mut words = line.split_whitespace();
        match words.next() {
            Some("VIS") => {
                // Keep the third field — it is the fader-scaling flag,
                // and it is not ours to decide.
                let _ = words.next();
                let _ = words.next();
                let rest = words.next().unwrap_or("1");
                out.push(format!(
                    "{indent}VIS {} {} {rest}",
                    if lane.visible { 1 } else { 0 },
                    if lane.in_own_lane { 1 } else { 0 },
                ));
            }
            Some("LANEHEIGHT") => {
                let _ = words.next();
                let rest = words.next().unwrap_or("0");
                out.push(format!("{indent}LANEHEIGHT {} {rest}", lane.height));
                wrote_height = true;
            }
            Some("ARM") => {
                out.push(format!("{indent}ARM {}", if lane.armed { 1 } else { 0 }));
            }
            Some("ACT") => {
                out.push(format!(
                    "{indent}ACT {} {}",
                    if lane.active { 1 } else { 0 },
                    lane.automation_mode
                ));
            }
            _ => out.push(line.to_string()),
        }
    }
    // A chunk that never carried a LANEHEIGHT gains one after VIS.
    if !wrote_height {
        if let Some(at) = out.iter().position(|l| l.trim_start().starts_with("VIS ")) {
            let indent: String = out[at].chars().take_while(|c| c.is_whitespace()).collect();
            out.insert(at + 1, format!("{indent}LANEHEIGHT {} 0", lane.height));
        }
    }
    let mut text = out.join("\n");
    if chunk.ends_with('\n') {
        text.push('\n');
    }
    text
}

#[cfg(test)]
mod lane_tests {
    use super::*;

    /// A chunk in REAPER's own shape, with a nested parameter envelope
    /// carrying its own VIS — the thing a naive parse gets wrong.
    fn chunk() -> String {
        [
            "<PARMENV 2 0 1 1",
            "  ACT 1 -1",
            "  VIS 1 0 1",
            "  LANEHEIGHT 0 0",
            "  ARM 1",
            "  DEFSHAPE 0 -1 -1",
            "  PT 0 0.5 0",
            ">",
        ]
        .join("\n")
    }

    #[test]
    fn the_lane_facts_come_off_the_chunk() {
        let state = parse_lane_state(&chunk());
        assert!(state.visible);
        assert!(!state.in_own_lane);
        assert_eq!(state.height, 0);
    }

    /// Splicing changes the lines it owns and nothing else — the points
    /// and the default shape survive byte for byte, and a field the
    /// caller did not touch comes back out as it went in.
    ///
    /// Written as parse-modify-splice, which is how every caller uses
    /// it: building a `LaneState` from `Default` would silently disarm
    /// an armed envelope, because this splice owns `ARM` too.
    #[test]
    fn splicing_touches_only_its_own_lines() {
        let mut state = parse_lane_state(&chunk());
        state.in_own_lane = true;
        state.height = 44;
        let out = splice_lane_state(&chunk(), state);

        assert!(out.contains("VIS 1 1 1"), "{out}");
        assert!(out.contains("LANEHEIGHT 44 0"), "{out}");
        assert!(
            out.contains("PT 0 0.5 0"),
            "the points were dropped:\n{out}"
        );
        assert!(
            out.contains("DEFSHAPE 0 -1 -1"),
            "the shape was dropped:\n{out}"
        );
        // Untouched by this caller, so unchanged in the output.
        assert!(
            out.contains("ARM 1"),
            "an armed envelope was disarmed:\n{out}"
        );
        assert!(out.contains("ACT 1 -1"), "the active flag moved:\n{out}");
        assert_eq!(out.lines().count(), chunk().lines().count());

        // And the read is the inverse of the write.
        let back = parse_lane_state(&out);
        assert!(back.in_own_lane);
        assert_eq!(back.height, 44);
        assert!(back.armed);
    }

    /// Arm, automation mode and the active flag round-trip too — the
    /// four facts that only exist in this chunk.
    #[test]
    fn arm_and_mode_round_trip() {
        let state = parse_lane_state(&chunk());
        assert!(state.armed, "ARM 1 did not read as armed");
        assert!(state.active, "ACT's first field did not read as active");
        assert_eq!(state.automation_mode, -1, "the mode is 'follow the track'");

        let out = splice_lane_state(
            &chunk(),
            LaneState {
                armed: false,
                automation_mode: 2,
                active: true,
                ..state
            },
        );
        assert!(out.contains("ARM 0"), "{out}");
        assert!(out.contains("ACT 1 2"), "{out}");
        let back = parse_lane_state(&out);
        assert!(!back.armed);
        assert_eq!(back.automation_mode, 2);
    }

    /// An envelope chunk with no ACT line is active and follows the
    /// track — reading it as inactive would deactivate it on the next
    /// rewrite.
    #[test]
    fn a_chunk_without_act_stays_active() {
        let state = parse_lane_state("<PARMENV 2 0 1 1\n  VIS 1 0 1\n>");
        assert!(state.active);
        assert_eq!(state.automation_mode, -1);
    }

    /// A chunk with no LANEHEIGHT gains one rather than losing the
    /// height silently.
    #[test]
    fn a_missing_laneheight_is_inserted() {
        let bare = "<PARMENV 2 0 1 1\n  VIS 1 0 1\n  PT 0 0.5 0\n>";
        let out = splice_lane_state(
            bare,
            LaneState {
                visible: true,
                in_own_lane: true,
                height: 30,
                ..Default::default()
            },
        );
        assert!(out.contains("LANEHEIGHT 30 0"), "{out}");
        assert!(out.contains("PT 0 0.5 0"));
    }
}

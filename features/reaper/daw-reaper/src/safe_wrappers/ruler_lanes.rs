//! Safe wrappers for REAPER ruler lane APIs (v7.62+).
//!
//! These APIs are only available in REAPER 7.62 and later. Functions check
//! for availability at runtime and return `None`/`false` when unavailable.

use super::ReaperLow;
use reaper_medium::ProjectContext;
use std::collections::HashMap;
use std::ffi::CString;
use std::ptr;
use std::sync::{Mutex, OnceLock};

type LaneKey = (bool, u32);
const EXT_STATE_SECTION: &str = "fasttrackstudio.ruler_lanes";

fn assigned_lanes() -> &'static Mutex<HashMap<LaneKey, u32>> {
    static LANES: OnceLock<Mutex<HashMap<LaneKey, u32>>> = OnceLock::new();
    LANES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Remember the lane DAW assigned through REAPER's setter.
///
/// REAPER's `I_LANENUMBER` can be set, but its getter returns a display-derived
/// value, so DAW keeps the assigned value for service reads in the same session.
pub fn remember_assigned_lane(
    low: &ReaperLow,
    project: ProjectContext,
    is_region: bool,
    id: u32,
    lane: u32,
) {
    if let Ok(mut lanes) = assigned_lanes().lock() {
        lanes.insert((is_region, id), lane);
    }

    let Ok(section) = CString::new(EXT_STATE_SECTION) else {
        return;
    };
    let Ok(key) = CString::new(lane_key(is_region, id)) else {
        return;
    };
    let Ok(value) = CString::new(lane.to_string()) else {
        return;
    };

    unsafe {
        low.SetProjExtState(
            project.to_raw(),
            section.as_ptr(),
            key.as_ptr(),
            value.as_ptr(),
        );
    }
}

pub fn assigned_lane(
    low: &ReaperLow,
    project: ProjectContext,
    is_region: bool,
    id: u32,
) -> Option<u32> {
    if let Some(lane) = assigned_lanes()
        .lock()
        .ok()
        .and_then(|lanes| lanes.get(&(is_region, id)).copied())
    {
        return Some(lane);
    }

    let section = CString::new(EXT_STATE_SECTION).ok()?;
    let key = CString::new(lane_key(is_region, id)).ok()?;
    let value = super::buffer::with_string_buffer_i32(64, |buf, len| unsafe {
        low.GetProjExtState(project.to_raw(), section.as_ptr(), key.as_ptr(), buf, len)
    })?;
    let lane = value.parse::<u32>().ok()?;

    if let Ok(mut lanes) = assigned_lanes().lock() {
        lanes.insert((is_region, id), lane);
    }

    Some(lane)
}

/// Carry a lane assignment from one number to another.
///
/// The shadow is keyed on REAPER's marker/region number, and REAPER's
/// "Renumber ... in timeline order" action reassigns those. Without
/// this, a renumber leaves every assignment filed under a number that
/// now belongs to something else — so the lane a region was PUT in
/// silently becomes another region's lane, and FTS reads the ruler
/// wrong from then on.
///
/// That matters more than it sounds: the SECTIONS lane is what makes a
/// region a song section. A lane lost in a renumber is a verse that
/// stops being a verse.
pub fn carry_assigned_lane(
    low: &ReaperLow,
    project: ProjectContext,
    is_region: bool,
    from: u32,
    to: u32,
) {
    let Some(lane) = assigned_lane(low, project, is_region, from) else {
        return;
    };
    remember_assigned_lane(low, project, is_region, to, lane);
    forget_assigned_lane(low, project, is_region, from);
}

/// Drop a lane assignment, in memory and in the project.
///
/// Called when the number it is filed under stops meaning what it
/// meant — a renumbering, or the marker going away. Leaving it behind
/// would hand the stale lane to whatever takes that number next.
pub fn forget_assigned_lane(low: &ReaperLow, project: ProjectContext, is_region: bool, id: u32) {
    if let Ok(mut lanes) = assigned_lanes().lock() {
        lanes.remove(&(is_region, id));
    }
    let (Ok(section), Ok(key), Ok(empty)) = (
        CString::new(EXT_STATE_SECTION),
        CString::new(lane_key(is_region, id)),
        CString::new(""),
    ) else {
        return;
    };
    // REAPER clears a key by setting it empty; there is no delete.
    unsafe {
        low.SetProjExtState(
            project.to_raw(),
            section.as_ptr(),
            key.as_ptr(),
            empty.as_ptr(),
        );
    }
}

fn lane_key(is_region: bool, id: u32) -> String {
    let kind = if is_region { "region" } else { "marker" };
    format!("{kind}:{id}")
}

/// Returns `true` if the running REAPER version supports ruler lane APIs
/// (i.e., `GetRegionOrMarkerInfo_Value` is present).
pub fn supports_ruler_lanes() -> bool {
    let low = reaper_high::Reaper::get().medium_reaper().low();
    low.pointers().GetRegionOrMarkerInfo_Value.is_some()
}

/// Get the lane number for a marker/region by its enumeration index.
///
/// `idx` is the 0-based enumeration index (same as used with `EnumProjectMarkers3`).
/// Returns `None` if the API is unavailable or the marker doesn't exist.
///
/// **API quirk:** REAPER's docs note that `I_LANENUMBER` "can be set,
/// but returned value is read-only" — the value returned here is the
/// *displayed* lane index recomputed from layout, not the value most
/// recently passed to [`set_marker_lane`]. Don't use this to round-
/// trip user-set lane assignments.
pub fn get_marker_lane(low: &ReaperLow, project: ProjectContext, idx: u32) -> Option<u32> {
    let get_marker = low.pointers().GetRegionOrMarker?;
    let get_info = low.pointers().GetRegionOrMarkerInfo_Value?;

    let marker_ptr = unsafe { get_marker(project.to_raw(), idx as i32, ptr::null()) };
    if marker_ptr.is_null() {
        return None;
    }

    let lane = unsafe { get_info(project.to_raw(), marker_ptr, c"I_LANENUMBER".as_ptr()) };
    Some(lane as u32)
}

/// Set the lane number for a marker/region by its enumeration index.
///
/// `idx` is the 0-based enumeration index.
/// Returns `true` on success.
pub fn set_marker_lane(low: &ReaperLow, project: ProjectContext, idx: u32, lane: u32) -> bool {
    let get_marker = match low.pointers().GetRegionOrMarker {
        Some(f) => f,
        None => return false,
    };
    let set_info = match low.pointers().SetRegionOrMarkerInfo_Value {
        Some(f) => f,
        None => return false,
    };

    let marker_ptr = unsafe { get_marker(project.to_raw(), idx as i32, ptr::null()) };
    if marker_ptr.is_null() {
        return false;
    }

    unsafe {
        set_info(
            project.to_raw(),
            marker_ptr,
            c"I_LANENUMBER".as_ptr(),
            lane as f64,
        )
    };
    low.UpdateTimeline();
    true
}

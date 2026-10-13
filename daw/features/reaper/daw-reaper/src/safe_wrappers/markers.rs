//! Safe wrappers for REAPER marker and region APIs.

use super::ReaperLow;
use reaper_medium::ProjectContext;
use std::ffi::CStr;
use std::os::raw::c_char;

/// Delete a project marker or region.
pub fn delete_project_marker(low: &ReaperLow, project: ProjectContext, id: i32, is_region: bool) {
    unsafe {
        low.DeleteProjectMarker(project.to_raw(), id, is_region);
    }
}

/// Set a project marker/region position and name.
///
/// Pass `name` as `None` to keep the existing name, or `-1.0` for
/// position/end to keep the existing values.
pub fn set_project_marker(
    low: &ReaperLow,
    id: i32,
    is_region: bool,
    pos: f64,
    end: f64,
    name: Option<&CStr>,
) {
    let name_ptr = name.map_or(std::ptr::null(), |n| n.as_ptr());
    unsafe {
        low.SetProjectMarker(id, is_region, pos, end, name_ptr);
    }
}

/// Set a project marker/region by index with color.
#[allow(clippy::too_many_arguments)]
pub fn set_project_marker_by_index2(
    low: &ReaperLow,
    project: ProjectContext,
    id: i32,
    is_region: bool,
    pos: f64,
    end: f64,
    name_idx: i32,
    name: Option<&CStr>,
    color: i32,
    flags: i32,
) {
    let name_ptr = name.map_or(std::ptr::null(), |n| n.as_ptr());
    unsafe {
        low.SetProjectMarkerByIndex2(
            project.to_raw(),
            id,
            is_region,
            pos,
            end,
            name_idx,
            name_ptr,
            color,
            flags,
        );
    }
}

/// Enumerate project markers. Returns `None` when no more markers exist.
pub fn enum_project_markers(low: &ReaperLow, idx: i32) -> Option<EnumMarkerResult> {
    let mut is_region = false;
    let mut pos = 0.0;
    let mut end = 0.0;
    let mut name_ptr: *const c_char = std::ptr::null();
    let mut marker_idx = 0;
    let result = unsafe {
        low.EnumProjectMarkers(
            idx,
            &mut is_region,
            &mut pos,
            &mut end,
            &mut name_ptr,
            &mut marker_idx,
        )
    };
    if result == 0 {
        return None;
    }
    Some(EnumMarkerResult {
        is_region,
        pos,
        end,
        name: super::cstring::read_cstr_or_empty(name_ptr),
        marker_idx,
    })
}

/// Result from enumerating project markers.
pub struct EnumMarkerResult {
    pub is_region: bool,
    pub pos: f64,
    pub end: f64,
    pub name: String,
    pub marker_idx: i32,
}

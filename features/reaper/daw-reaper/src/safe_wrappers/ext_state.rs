//! Safe wrappers for REAPER ExtState APIs.

use super::ReaperLow;
use reaper_medium::ProjectContext;
use std::ffi::CString;

/// Get a global ext state value.
pub fn get_ext_state(low: &ReaperLow, section: &CString, key: &CString) -> Option<String> {
    let ptr = unsafe { low.GetExtState(section.as_ptr(), key.as_ptr()) };
    super::cstring::read_cstr(ptr)
}

/// Set a global ext state value.
pub fn set_ext_state(
    low: &ReaperLow,
    section: &CString,
    key: &CString,
    value: &CString,
    persist: bool,
) {
    unsafe {
        low.SetExtState(section.as_ptr(), key.as_ptr(), value.as_ptr(), persist);
    }
}

/// Delete a global ext state value.
pub fn delete_ext_state(low: &ReaperLow, section: &CString, key: &CString, persist: bool) {
    unsafe {
        low.DeleteExtState(section.as_ptr(), key.as_ptr(), persist);
    }
}

/// Check if a global ext state value exists.
pub fn has_ext_state(low: &ReaperLow, section: &CString, key: &CString) -> bool {
    unsafe { low.HasExtState(section.as_ptr(), key.as_ptr()) }
}

/// Largest project ext-state value we will read back. Project-scoped state is
/// meant to be small and textual, but "small" is a convention, not a limit —
/// this is a sanity ceiling, not an expected size.
const MAX_PROJ_EXT_STATE: usize = 16 << 20;

/// Get a project-scoped ext state value.
///
/// `buf_size` is only the *starting* guess: `GetProjExtState` silently truncates
/// to whatever buffer it is handed, so the read grows and retries rather than
/// returning a short string. See [`super::buffer::with_growing_string_buffer_i32`].
pub fn get_proj_ext_state(
    low: &ReaperLow,
    project: ProjectContext,
    section: &CString,
    key: &CString,
    buf_size: usize,
) -> Option<String> {
    super::buffer::with_growing_string_buffer_i32(buf_size, MAX_PROJ_EXT_STATE, |buf, len| unsafe {
        low.GetProjExtState(project.to_raw(), section.as_ptr(), key.as_ptr(), buf, len)
    })
}

/// Set a project-scoped ext state value.
pub fn set_proj_ext_state(
    low: &ReaperLow,
    project: ProjectContext,
    section: &CString,
    key: &CString,
    value: &CString,
) {
    unsafe {
        low.SetProjExtState(
            project.to_raw(),
            section.as_ptr(),
            key.as_ptr(),
            value.as_ptr(),
        );
    }
}

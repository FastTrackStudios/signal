//! Safe wrappers for REAPER audio device and project info APIs.

use std::ffi::CStr;

use reaper_medium::ReaProject;

use super::ReaperLow;

/// Query an audio device info string (e.g. `"IDENT_IN"`, `"SRATE"`).
pub fn get_audio_device_info(low: &ReaperLow, attr: &CStr, buf_size: usize) -> Option<String> {
    super::buffer::with_string_buffer(buf_size, |buf, len| unsafe {
        low.GetAudioDeviceInfo(attr.as_ptr(), buf, len)
    })
}

/// Query a floating-point project info value (e.g. `"PROJECT_SRATE"`).
pub fn get_set_project_info(
    low: &ReaperLow,
    project: ReaProject,
    attr: &CStr,
    value: f64,
    is_set: bool,
) -> f64 {
    unsafe { low.GetSetProjectInfo(project.as_ptr(), attr.as_ptr(), value, is_set) }
}

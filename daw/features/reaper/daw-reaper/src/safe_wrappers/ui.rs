//! Safe wrappers for REAPER's blocking file/directory browser dialogs.
//!
//! All three functions block the REAPER main thread while the system
//! file dialog is open — they MUST be invoked from a `main_thread::run`
//! / `main_thread::query` context, not a background task.

use std::ffi::CString;
use std::path::{Path, PathBuf};

use reaper_low::Swell;

/// Convert an `Option<&Path>` to a CString. Empty CString if None /
/// non-UTF-8 (REAPER treats empty as "no preference").
fn path_to_cstring(path: Option<&Path>) -> CString {
    let s = path
        .and_then(|p| p.to_str())
        .map(|s| s.to_string())
        .unwrap_or_default();
    // Strip interior nulls so CString::new can't fail.
    let cleaned: String = s.chars().filter(|c| *c != '\0').collect();
    CString::new(cleaned).unwrap_or_default()
}

fn str_to_cstring(s: &str) -> CString {
    let cleaned: String = s.chars().filter(|c| *c != '\0').collect();
    CString::new(cleaned).unwrap_or_default()
}

/// Default extension list for REAPER's file dialogs. Format follows
/// REAPER's docs: comma-separated `"label\0glob\0label\0glob\0\0"`.
/// Caller passes a preformatted string; we convert to a null-terminated
/// CString here. Empty filter → all-files.
fn filter_to_cstring(filter: Option<&str>) -> CString {
    str_to_cstring(filter.unwrap_or(""))
}

/// `BrowseForFiles` with `allowmul=false` — returns the single chosen
/// path, or `None` if the user cancelled. The returned buffer is owned
/// by REAPER (valid until the next call) so we copy into an owned
/// `PathBuf` immediately.
pub fn browse_for_open_file(
    swell: &Swell,
    title: &str,
    initial_dir: Option<&Path>,
    filter: Option<&str>,
) -> Option<PathBuf> {
    let title_c = str_to_cstring(title);
    let dir_c = path_to_cstring(initial_dir);
    let init_file_c = CString::default();
    let filter_c = filter_to_cstring(filter);

    let raw = unsafe {
        swell.BrowseForFiles(
            title_c.as_ptr(),
            dir_c.as_ptr(),
            init_file_c.as_ptr(),
            false,
            filter_c.as_ptr(),
        )
    };
    if raw.is_null() {
        return None;
    }
    let cstr = unsafe { std::ffi::CStr::from_ptr(raw) };
    let s = cstr.to_str().ok()?.to_string();
    if s.is_empty() {
        None
    } else {
        Some(PathBuf::from(s))
    }
}

/// `BrowseForSaveFile` — returns the user's chosen save path, or `None`
/// on cancel. `default_name` pre-fills the filename field.
pub fn browse_for_save_file(
    swell: &Swell,
    title: &str,
    initial_dir: Option<&Path>,
    default_name: &str,
    filter: Option<&str>,
) -> Option<PathBuf> {
    let title_c = str_to_cstring(title);
    let dir_c = path_to_cstring(initial_dir);
    let default_c = str_to_cstring(default_name);
    let filter_c = filter_to_cstring(filter);

    let mut buf = vec![0i8; 4096];
    let ok = unsafe {
        swell.BrowseForSaveFile(
            title_c.as_ptr(),
            dir_c.as_ptr(),
            default_c.as_ptr(),
            filter_c.as_ptr(),
            buf.as_mut_ptr(),
            buf.len() as i32,
        )
    };
    if !ok {
        return None;
    }
    let cstr = unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) };
    cstr.to_str().ok().and_then(|s| {
        if s.is_empty() {
            None
        } else {
            Some(PathBuf::from(s))
        }
    })
}

/// `BrowseForDirectory` — returns the user's chosen folder, or `None`
/// on cancel.
pub fn browse_for_directory(
    swell: &Swell,
    title: &str,
    initial_dir: Option<&Path>,
) -> Option<PathBuf> {
    let title_c = str_to_cstring(title);
    let dir_c = path_to_cstring(initial_dir);

    let mut buf = vec![0i8; 4096];
    let ok = unsafe {
        swell.BrowseForDirectory(
            title_c.as_ptr(),
            dir_c.as_ptr(),
            buf.as_mut_ptr(),
            buf.len() as i32,
        )
    };
    if !ok {
        return None;
    }
    let cstr = unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) };
    cstr.to_str().ok().and_then(|s| {
        if s.is_empty() {
            None
        } else {
            Some(PathBuf::from(s))
        }
    })
}

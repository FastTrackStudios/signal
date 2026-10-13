//! Safe helpers for reading C strings from REAPER pointers.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

/// Read a NUL-terminated C string from a raw pointer, returning `None` if null
/// or empty.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn read_cstr(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let s = unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned();
    if s.is_empty() { None } else { Some(s) }
}

/// Read a NUL-terminated C string from a raw pointer, returning empty string if
/// null. Unlike [`read_cstr`], this never returns `None`.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn read_cstr_or_empty(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

/// Convert a Rust string to a CString, returning `None` if it contains
/// interior NUL bytes.
pub fn to_cstring(s: impl Into<Vec<u8>>) -> Option<CString> {
    CString::new(s).ok()
}

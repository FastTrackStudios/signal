//! Safe wrappers for REAPER FX APIs.
//!
//! Wraps unsafe low-level and medium-level FX calls so that `fx.rs`
//! service code can be 100% safe Rust.

use super::ReaperLow;
use reaper_medium::{MediaTrack, TrackFxLocation, TransferBehavior};
use std::ffi::CString;
use std::os::raw::c_char;

// =============================================================================
// Low-level wrappers (via `low`)
// =============================================================================

/// Enumerate installed FX by index. Returns `(name, ident)` or `None` when
/// the index is past the end of the list.
pub fn enum_installed_fx(low: &ReaperLow, index: i32) -> Option<(String, String)> {
    let mut name_ptr: *const c_char = std::ptr::null();
    let mut ident_ptr: *const c_char = std::ptr::null();
    let ok = unsafe { low.EnumInstalledFX(index, &mut name_ptr, &mut ident_ptr) };
    if !ok {
        return None;
    }
    let name = super::cstring::read_cstr_or_empty(name_ptr);
    let ident = super::cstring::read_cstr_or_empty(ident_ptr);
    Some((name, ident))
}

/// Read FX output pin mappings for a given pin.
/// Returns `(low32, high32)` bitmask pair.
pub fn track_fx_get_pin_mappings(
    low: &ReaperLow,
    track: MediaTrack,
    fx: i32,
    is_output: i32,
    pin: i32,
) -> (i32, i32) {
    let mut high32: i32 = 0;
    let low32 =
        unsafe { low.TrackFX_GetPinMappings(track.as_ptr(), fx, is_output, pin, &mut high32) };
    (low32, high32)
}

/// Set FX output pin mappings for a given pin.
pub fn track_fx_set_pin_mappings(
    low: &ReaperLow,
    track: MediaTrack,
    fx: i32,
    is_output: i32,
    pin: i32,
    low32: i32,
    high32: i32,
) {
    unsafe {
        low.TrackFX_SetPinMappings(track.as_ptr(), fx, is_output, pin, low32, high32);
    }
}

// =============================================================================
// Medium-level wrappers
// =============================================================================

/// Copy/move an FX within or between tracks.
pub fn track_fx_copy_to_track(
    medium: &reaper_medium::Reaper,
    from: (MediaTrack, TrackFxLocation),
    to: (MediaTrack, TrackFxLocation),
    behavior: TransferBehavior,
) {
    unsafe {
        medium.track_fx_copy_to_track(from, to, behavior);
    }
}

/// Delete an FX from a track.
pub fn track_fx_delete(
    medium: &reaper_medium::Reaper,
    track: MediaTrack,
    location: TrackFxLocation,
) -> Result<(), reaper_medium::ReaperFunctionError> {
    unsafe { medium.track_fx_delete(track, location) }
}

/// Navigate FX presets by delta (+1 = next, -1 = prev).
pub fn track_fx_navigate_presets(
    medium: &reaper_medium::Reaper,
    track: MediaTrack,
    location: TrackFxLocation,
    delta: i32,
) {
    unsafe {
        let _ = medium.track_fx_navigate_presets(track, location, delta);
    }
}

/// Set FX preset by index.
pub fn track_fx_set_preset_by_index(
    medium: &reaper_medium::Reaper,
    track: MediaTrack,
    location: TrackFxLocation,
    preset_ref: reaper_medium::FxPresetRef,
) {
    unsafe {
        let _ = medium.track_fx_set_preset_by_index(track, location, preset_ref);
    }
}

// =============================================================================
// High-level wrappers (reaper_high::Fx)
// =============================================================================

/// Set a named config parameter on an FX instance.
///
/// Wraps the unsafe `Fx::set_named_config_param` by building a `CString`
/// internally so callers don't need `unsafe`.
pub fn fx_set_named_config_param(
    fx: &reaper_high::Fx,
    key: &str,
    value: &str,
) -> Result<(), String> {
    let c_value = CString::new(value)
        .map_err(|e| format!("fx_set_named_config_param: invalid value: {}", e))?;
    unsafe {
        fx.set_named_config_param(key, c_value.as_ptr())
            .map_err(|e| format!("fx_set_named_config_param failed: {}", e))
    }
}

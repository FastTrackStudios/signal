//! Mouse Utility
//!
//! Simplified mouse utilities - re-exports from window_detection module.
//!
//! This module provides backwards compatibility for code that was using
//! the old mouse_util functions. New code should use window_detection directly.

use crate::input::state::Context;
use crate::input::window_detection::{self};
use reaper_low::raw::HWND;
use reaper_medium::Reaper as MediumReaper;

// region: --- Re-exports

/// Check if an HWND is a MIDI editor window by walking up the parent chain
/// Returns (midi_editor_hwnd, subview_hwnd) if found
///
/// This is a compatibility wrapper around window_detection::is_hwnd_midi_editor
pub fn is_hwnd_midi_editor(
    hwnd: HWND,
    medium_reaper: &MediumReaper,
) -> Option<(HWND, Option<HWND>)> {
    window_detection::is_hwnd_midi_editor(hwnd, medium_reaper).map(|info| (info.hwnd, info.subview))
}

/// Get mouse position in screen coordinates
pub fn get_mouse_position(medium_reaper: &MediumReaper) -> (i32, i32) {
    window_detection::get_mouse_position(medium_reaper)
}

/// Get the window under the mouse cursor
pub fn get_window_under_mouse(mouse_x: i32, mouse_y: i32) -> HWND {
    window_detection::get_window_under_mouse(mouse_x, mouse_y)
}

/// Determine context from mouse position (not keyboard focus)
/// This is the main function that should be used for wheel events
///
/// Returns (Context, context_name, window_title)
pub fn determine_context_from_mouse_position(
    medium_reaper: &MediumReaper,
) -> (Context, String, String) {
    window_detection::detect_context_from_mouse_compat(medium_reaper)
}

// endregion: --- Re-exports

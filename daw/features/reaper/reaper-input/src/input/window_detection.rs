//! Window Detection
//!
//! Consolidated window detection utilities for the input module.
//! Provides functions to detect which REAPER window the mouse is over
//! or which window has keyboard focus.
//!
//! This module consolidates duplicate detection code from:
//! - handler.rs (determine_context)
//! - mouse_util.rs (is_hwnd_midi_editor, determine_context_from_hwnd)
//! - wheel_hook.rs (determine_context_from_hwnd)

use crate::input::state::Context;
use reaper_low::Swell;
use reaper_low::raw::{HWND, POINT};
use reaper_medium::Reaper as MediumReaper;
use swell_ui::Window;

// region: --- Panel context probe

/// Host-registered probe: does this HWND belong to an app-defined input
/// context? Returns the context's tag when it does.
///
/// The dock's panels are ordinary SWELL children the detection code
/// here knows nothing about; the extension registers a probe that asks
/// the dock, so a focused panel can claim its own [`Context`] instead
/// of falling through to `Main` (which is what let every global REAPER
/// binding keep firing while typing at the expression editor).
static PANEL_CONTEXT_PROBE: std::sync::OnceLock<fn(HWND) -> Option<&'static str>> =
    std::sync::OnceLock::new();

/// The tag [`Context::ExpressionEditor`] is keyed by, in probes and in
/// keybind config (`context @Custom "expression-editor"`).
pub const EXPRESSION_EDITOR_TAG: &str = "expression-editor";

/// Register the probe. First caller wins.
pub fn set_panel_context_probe(f: fn(HWND) -> Option<&'static str>) {
    let _ = PANEL_CONTEXT_PROBE.set(f);
}

/// The app-defined context claiming `hwnd`, if any.
pub(crate) fn panel_context_of(hwnd: HWND) -> Option<Context> {
    let probe = PANEL_CONTEXT_PROBE.get()?;
    match probe(hwnd)? {
        EXPRESSION_EDITOR_TAG => Some(Context::ExpressionEditor),
        _ => None,
    }
}

// endregion: --- Panel context probe

// region: --- Types

/// MIDI editor mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidiEditorMode {
    /// Piano roll view (mode 0)
    PianoRoll,
    /// Event list view (mode 1)
    EventList,
    /// Unknown mode
    Unknown,
}

/// Result of MIDI editor detection
#[derive(Debug, Clone)]
pub struct MidiEditorInfo {
    /// The MIDI editor HWND
    pub hwnd: HWND,
    /// The subview HWND (if mouse is over a child window)
    pub subview: Option<HWND>,
    /// The editor mode
    pub mode: MidiEditorMode,
}

/// Window context detection result
#[derive(Debug, Clone)]
pub struct WindowContext {
    /// The detected context
    pub context: Context,
    /// Human-readable context name
    pub context_name: String,
    /// Window title (for logging)
    pub window_title: String,
}

impl Default for WindowContext {
    fn default() -> Self {
        Self {
            context: Context::Main,
            context_name: "Main".to_string(),
            window_title: String::new(),
        }
    }
}

// endregion: --- Types

// region: --- Mouse Position

/// Get mouse position in screen coordinates
pub fn get_mouse_position(medium_reaper: &MediumReaper) -> (i32, i32) {
    let low_reaper = medium_reaper.low();
    let mut mouse_x: i32 = 0;
    let mut mouse_y: i32 = 0;
    unsafe {
        low_reaper.GetMousePosition(&mut mouse_x, &mut mouse_y);
    }
    (mouse_x, mouse_y)
}

/// Get the window under the mouse cursor
pub fn get_window_under_mouse(mouse_x: i32, mouse_y: i32) -> HWND {
    let mouse_point = POINT {
        x: mouse_x,
        y: mouse_y,
    };
    unsafe { Swell::get().WindowFromPoint(mouse_point) }
}

// endregion: --- Mouse Position

// region: --- MIDI Editor Detection

/// Check if an HWND is a MIDI editor window by walking up the parent chain
///
/// Returns Some(MidiEditorInfo) if the window is part of a MIDI editor,
/// None otherwise.
pub fn is_hwnd_midi_editor(hwnd: HWND, medium_reaper: &MediumReaper) -> Option<MidiEditorInfo> {
    if hwnd.is_null() {
        return None;
    }

    // Check if this window itself is a MIDI editor
    let mode = unsafe { medium_reaper.low().MIDIEditor_GetMode(hwnd) };

    if mode != -1 {
        // This is a MIDI editor window
        return Some(MidiEditorInfo {
            hwnd,
            subview: None,
            mode: mode_to_enum(mode),
        });
    }

    // Walk up the parent chain to find a MIDI editor parent
    let mut current = hwnd;
    while !current.is_null() {
        let parent = get_parent_hwnd(current);

        if parent.is_null() {
            break;
        }

        // Check if parent is a MIDI editor
        let parent_mode = unsafe { medium_reaper.low().MIDIEditor_GetMode(parent) };

        if parent_mode != -1 {
            // Found MIDI editor parent, current is the subview
            return Some(MidiEditorInfo {
                hwnd: parent,
                subview: Some(current),
                mode: mode_to_enum(parent_mode),
            });
        }

        current = parent;
    }

    None
}

/// Convert MIDI editor mode integer to enum
fn mode_to_enum(mode: i32) -> MidiEditorMode {
    match mode {
        0 => MidiEditorMode::PianoRoll,
        1 => MidiEditorMode::EventList,
        _ => MidiEditorMode::Unknown,
    }
}

/// Get parent HWND using swell_ui::Window
fn get_parent_hwnd(hwnd: HWND) -> HWND {
    if let Some(window) = Window::new(hwnd) {
        window
            .parent()
            .map(|w| w.raw_hwnd().as_ptr())
            .unwrap_or(std::ptr::null_mut())
    } else {
        std::ptr::null_mut()
    }
}

// endregion: --- MIDI Editor Detection

// region: --- Window Title Matching

/// Check if window title matches Media Explorer
pub fn is_media_explorer_title(title: &str) -> bool {
    let title_lower = title.to_lowercase();
    title_lower.contains("media explorer") || title_lower.contains("mediaexplorer")
}

/// Check if window title matches Crossfade Editor
pub fn is_crossfade_editor_title(title: &str) -> bool {
    let title_lower = title.to_lowercase();
    title_lower.contains("crossfade") && title_lower.contains("editor")
}

/// Check if window title matches MIDI Inline Editor
pub fn is_inline_midi_editor_title(title: &str) -> bool {
    let title_lower = title.to_lowercase();
    (title_lower.contains("inline") || title_lower.contains("midi inline"))
        && (title_lower.contains("midi") || title_lower.contains("editor"))
}

// endregion: --- Window Title Matching

// region: --- Context Detection

/// Determine context from the window under the mouse cursor
///
/// This is the primary function for mouse-based context detection.
/// It uses GetMousePosition and WindowFromPoint to accurately detect
/// which window the mouse is over, independent of keyboard focus.
pub fn detect_context_from_mouse(medium_reaper: &MediumReaper) -> WindowContext {
    let (mouse_x, mouse_y) = get_mouse_position(medium_reaper);
    let hwnd = get_window_under_mouse(mouse_x, mouse_y);

    if hwnd.is_null() {
        return WindowContext::default();
    }

    detect_context_from_hwnd(hwnd, medium_reaper)
}

/// Determine context from a specific HWND
///
/// Walks up the parent chain to identify special windows like
/// MIDI editors, Media Explorer, Crossfade Editor, etc.
pub fn detect_context_from_hwnd(hwnd: HWND, medium_reaper: &MediumReaper) -> WindowContext {
    let mut result = WindowContext::default();

    if hwnd.is_null() {
        return result;
    }

    // App-defined panels first — a docked panel is a child of the main
    // window, so every later check would misfile it as `Main`.
    if let Some(context) = panel_context_of(hwnd) {
        if let Some(window) = Window::new(hwnd)
            && let Ok(title) = window.text()
        {
            result.window_title = title;
        }
        result.context_name = format!("{context:?}");
        result.context = context;
        return result;
    }

    // Check if it's a MIDI editor first
    if let Some(midi_info) = is_hwnd_midi_editor(hwnd, medium_reaper) {
        // Get window title
        if let Some(window) = Window::new(midi_info.hwnd)
            && let Ok(title) = window.text()
        {
            result.window_title = title;
        }

        match midi_info.mode {
            MidiEditorMode::PianoRoll => {
                result.context = Context::Midi;
                result.context_name = "MIDI Editor (Piano Roll)".to_string();
            }
            MidiEditorMode::EventList => {
                result.context = Context::MidiEventListEditor;
                result.context_name = "MIDI Event List Editor".to_string();
            }
            MidiEditorMode::Unknown => {
                result.context = Context::Midi;
                result.context_name = "MIDI Editor".to_string();
            }
        }
        return result;
    }

    // Try to create a Window from the HWND
    let window = match Window::new(hwnd) {
        Some(w) => w,
        None => return result,
    };

    // Check if this is the main window
    let main_hwnd = medium_reaper.get_main_hwnd();
    if window.raw_hwnd().as_ptr() == main_hwnd.as_ptr() {
        if let Ok(title) = window.text() {
            result.window_title = title;
        }
        result.context = Context::Main;
        result.context_name = "Main".to_string();
        return result;
    }

    // Check window title for special windows
    if let Ok(title) = window.text() {
        result.window_title = title.clone();

        if is_media_explorer_title(&title) {
            result.context = Context::MediaExplorer;
            result.context_name = "Media Explorer".to_string();
            return result;
        }

        if is_crossfade_editor_title(&title) {
            result.context = Context::CrossfadeEditor;
            result.context_name = "Crossfade Editor".to_string();
            return result;
        }

        if is_inline_midi_editor_title(&title) {
            result.context = Context::MidiInlineEditor;
            result.context_name = "MIDI Inline Editor".to_string();
            return result;
        }
    }

    // Walk up parent chain to check for special windows
    let mut current = window.parent();
    while let Some(w) = current {
        if let Ok(title) = w.text() {
            if result.window_title.is_empty() {
                result.window_title = title.clone();
            }

            if is_media_explorer_title(&title) {
                result.context = Context::MediaExplorer;
                result.context_name = "Media Explorer".to_string();
                return result;
            }

            if is_crossfade_editor_title(&title) {
                result.context = Context::CrossfadeEditor;
                result.context_name = "Crossfade Editor".to_string();
                return result;
            }

            if is_inline_midi_editor_title(&title) {
                result.context = Context::MidiInlineEditor;
                result.context_name = "MIDI Inline Editor".to_string();
                return result;
            }
        }
        current = w.parent();
    }

    // Default to Main
    result.context = Context::Main;
    result.context_name = "Main".to_string();
    result
}

/// Determine context from keyboard focus
///
/// This is used when we need to know the context based on which window
/// has keyboard focus, rather than where the mouse is.
pub fn detect_context_from_focus(medium_reaper: &MediumReaper) -> WindowContext {
    let mut result = WindowContext::default();

    let focused_window = match Window::focused() {
        Some(w) => w,
        None => return result,
    };

    let focused_hwnd = focused_window.raw_hwnd();

    // App-defined panels first, same as the hwnd path.
    if let Some(context) = panel_context_of(focused_hwnd.as_ptr()) {
        if let Ok(title) = focused_window.text() {
            result.window_title = title;
        }
        result.context_name = format!("{context:?}");
        result.context = context;
        return result;
    }

    // Get window title for logging
    if let Ok(title) = focused_window.text() {
        result.window_title = title;
    }

    // Detect a MIDI editor directly from the focused window by walking its
    // own parent chain with MIDIEditor_GetMode (same logic as the mouse
    // path). This is more reliable than trusting midi_editor_get_active(),
    // which tracks REAPER's single "active" editor and can disagree with the
    // actually-focused window when several MIDI editors are open (docked +
    // floating, two floats, etc.) — and it reads the mode from the focused
    // editor, so Piano Roll vs Event List is never taken from the wrong one.
    //
    // midi_editor_get_active() is kept only as a fallback for the case where
    // the focused HWND itself doesn't resolve to a MIDI editor but focus is
    // still nested under the active one (e.g. a transient child SWELL hasn't
    // reparented onto the editor yet).
    let midi_info = is_hwnd_midi_editor(focused_hwnd.as_ptr(), medium_reaper).or_else(|| {
        medium_reaper.midi_editor_get_active().and_then(|active| {
            is_window_child_of(focused_hwnd.as_ptr(), active.as_ptr())
                .then(|| is_hwnd_midi_editor(active.as_ptr(), medium_reaper))
                .flatten()
        })
    });

    if let Some(midi_info) = midi_info {
        if let Some(window) = Window::new(midi_info.hwnd)
            && let Ok(title) = window.text()
        {
            result.window_title = title;
        }

        match midi_info.mode {
            MidiEditorMode::EventList => {
                result.context = Context::MidiEventListEditor;
                result.context_name = "MIDI Event List Editor".to_string();
            }
            _ => {
                result.context = Context::Midi;
                result.context_name = "MIDI Editor".to_string();
            }
        }
        return result;
    }

    // Check for special windows by title
    if let Ok(title) = focused_window.text() {
        if is_media_explorer_title(&title) {
            result.context = Context::MediaExplorer;
            result.context_name = "Media Explorer".to_string();
            return result;
        }

        if is_crossfade_editor_title(&title) {
            result.context = Context::CrossfadeEditor;
            result.context_name = "Crossfade Editor".to_string();
            return result;
        }
    }

    // Walk up parent chain
    let mut current = Some(focused_window);
    while let Some(window) = current {
        if let Ok(title) = window.text() {
            if result.window_title.is_empty() {
                result.window_title = title.clone();
            }

            if is_media_explorer_title(&title) {
                result.context = Context::MediaExplorer;
                result.context_name = "Media Explorer".to_string();
                return result;
            }

            if is_crossfade_editor_title(&title) {
                result.context = Context::CrossfadeEditor;
                result.context_name = "Crossfade Editor".to_string();
                return result;
            }

            if is_inline_midi_editor_title(&title) {
                result.context = Context::MidiInlineEditor;
                result.context_name = "MIDI Inline Editor".to_string();
                return result;
            }
        }
        current = window.parent();
    }

    // Default to Main
    result.context = Context::Main;
    result.context_name = "Main Window".to_string();
    result
}

/// Check if a window is a child of another window (or the same window)
pub(crate) fn is_window_child_of(child: HWND, parent: HWND) -> bool {
    if child == parent {
        return true;
    }

    let mut current = child;
    while !current.is_null() {
        let p = get_parent_hwnd(current);
        if p == parent {
            return true;
        }
        if p.is_null() {
            break;
        }
        current = p;
    }

    false
}

// endregion: --- Context Detection

// region: --- Compatibility Functions

/// Compatibility wrapper that returns the tuple format used by existing code
///
/// Returns (Context, context_name, window_title)
pub fn detect_context_from_mouse_compat(medium_reaper: &MediumReaper) -> (Context, String, String) {
    let result = detect_context_from_mouse(medium_reaper);
    (result.context, result.context_name, result.window_title)
}

/// Compatibility wrapper for focus-based detection
///
/// Returns (Context, context_name, window_title)
pub fn detect_context_from_focus_compat(medium_reaper: &MediumReaper) -> (Context, String, String) {
    let result = detect_context_from_focus(medium_reaper);
    (result.context, result.context_name, result.window_title)
}

// endregion: --- Compatibility Functions

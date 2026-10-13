//! Mouse Context Detection
//!
//! Full port of BR_MouseInfo from BR_MouseUtil.cpp (SWS).
//! Determines the context (window, segment, details) where the mouse cursor is located.
//! Uses GetMousePosition and WindowFromPoint to accurately detect context independent
//! of keyboard focus.
//!
//! ## Implemented Functionality
//!
//! ✅ **Core Infrastructure:**
//! - Mouse position detection (GetMousePosition, WindowFromPoint)
//! - Window hierarchy traversal
//! - MIDI editor detection via parent chain walking
//! - Detection mode flags for performance optimization
//!
//! ✅ **Window Detection:**
//! - Main window detection
//! - MIDI editor detection (piano roll, event list modes)
//! - Media Explorer detection (via window title)
//! - Crossfade Editor detection (via window title)
//! - Inline MIDI detection (via window title)
//!
//! ✅ **Ruler Detection:**
//! - Ruler window detection (stub - needs GetRulerWndAlt from SWS)
//! - Lane height calculation (region_lane, marker_lane, tempo_lane, timeline)
//!
//! ⚠️ **Partially Implemented (needs SWS functions):**
//! - Transport window detection (stub - needs GetTransportWnd from SWS)
//! - TCP/MCP detection (fallback via window title - needs HwndToTrack from SWS)
//! - Arrange view detection (basic - needs GetArrangeWnd, GetItemFromPoint from SWS)
//! - Track/envelope detection (stub - needs GetTrackOrEnvelopeFromY, HwndToEnvelope from SWS)
//! - Item detection (stub - needs GetItemFromPoint from REAPER API)
//! - Stretch marker detection (stub - needs GetTakeHeight, stretch marker APIs)
//! - Envelope point/segment detection (stub - needs BR_Envelope class from SWS)
//! - MIDI editor segment detection (ruler, piano, notes, CC lanes - needs BR_MidiEditor from SWS)
//! - CC lane value detection (stub - needs BR_MidiEditor::GetCCLanesFullheight, etc.)
//! - Note row detection (stub - needs BR_MidiEditor::GetVZoom, GetVPos, etc.)
//! - Inline MIDI detection (stub - needs GetTakeHeight, IsOpenInInlineEditor from SWS)
//!
//! ## Dependencies on SWS Functions
//!
//! The following SWS-specific functions would need to be implemented or bridged:
//! - `GetArrangeWnd()` - Get arrange view window HWND
//! - `GetRulerWndAlt()` - Get ruler window HWND
//! - `GetTransportWnd()` - Get transport window HWND
//! - `HwndToTrack()` - Convert HWND to MediaTrack* (TCP/MCP)
//! - `HwndToEnvelope()` - Convert HWND to TrackEnvelope*
//! - `GetItemFromPoint()` - Get MediaItem* at screen point (REAPER API v5.975+)
//! - `GetTakeHeight()` - Get take height and offset
//! - `IsOpenInInlineEditor()` - Check if take is open in inline editor
//! - `GetNotesView()` - Get notes view HWND from MIDI editor
//! - `GetPianoView()` - Get piano view HWND from MIDI editor
//! - `BR_Envelope` class - Envelope manipulation and querying
//! - `BR_MidiEditor` class - MIDI editor state querying
//!
//! ## Current Status
//!
//! The current implementation provides:
//! 1. Basic window detection (main, MIDI editor, Media Explorer, Crossfade Editor)
//! 2. MIDI editor mode detection (piano roll vs event list)
//! 3. Window title-based fallbacks for special windows
//! 4. Framework for full implementation when SWS functions are available
//!
//! For production use, the SWS-specific functions would need to be either:
//! - Implemented as FFI bindings to SWS
//! - Re-implemented using REAPER's public APIs where possible
//! - Approximated using available REAPER APIs

use crate::input::constants::{
    MIDI_WND_KEYBOARD, MIDI_WND_NOTEVIEW, MIDI_WND_UNKNOWN, SCROLLBAR_W,
};
use crate::input::midi_utils;
use crate::input::reaper_windows;
use crate::input::state::Context;
use crate::input::utils;
use reaper_low::Swell;
use reaper_low::raw::{HWND, MediaItem_Take, POINT, RECT};
use reaper_medium::Reaper as MediumReaper;
use swell_ui::Window;

/// Detection mode flags (bitmask)
#[derive(Debug, Clone, Copy)]
pub struct DetectionMode {
    pub all: bool,
    pub ruler: bool,
    pub transport: bool,
    pub mcp_tcp: bool,
    pub arrange: bool,
    pub midi_editor: bool,
    pub midi_inline: bool,
    pub ignore_track_lane_elements_but_items: bool,
    pub ignore_envelope_lane_segment: bool,
}

impl Default for DetectionMode {
    fn default() -> Self {
        Self {
            all: true,
            ruler: true,
            transport: true,
            mcp_tcp: true,
            arrange: true,
            midi_editor: true,
            midi_inline: true,
            ignore_track_lane_elements_but_items: false,
            ignore_envelope_lane_segment: false,
        }
    }
}

impl DetectionMode {
    pub fn all() -> Self {
        Self::default()
    }

    pub fn minimal() -> Self {
        Self {
            all: false,
            ruler: false,
            transport: false,
            mcp_tcp: false,
            arrange: true,
            midi_editor: true,
            midi_inline: false,
            ignore_track_lane_elements_but_items: false,
            ignore_envelope_lane_segment: false,
        }
    }
}

/// Mouse context information
#[derive(Debug, Clone)]
pub struct MouseContext {
    /// The window type (unknown, ruler, transport, tcp, mcp, arrange, midi_editor)
    pub window: String,
    /// The segment within the window (region_lane, marker_lane, tempo_lane, timeline, track, envelope, notes, piano, cc_lane, etc.)
    pub segment: String,
    /// Additional details (empty, item, item_stretch_marker, env_point, env_segment, spacer, cc_selector, cc_lane)
    pub details: String,
    /// The detected REAPER context
    pub context: Context,
    /// Window title for logging
    pub window_title: String,
    /// Time position in arrange or MIDI ruler (returns -1 if not applicable)
    pub position: f64,
    /// MIDI editor HWND if in MIDI editor
    pub midi_editor_hwnd: Option<HWND>,
    /// Whether this is inline MIDI
    pub inline_midi: bool,
    /// Note row (-1 if not over any note row)
    pub note_row: i32,
    /// CC lane info (-2 if not over CC lane, -1 for velocity lane)
    pub cc_lane: i32,
    pub cc_lane_val: i32,
    pub cc_lane_id: i32,
    /// Piano roll mode (0=normal, 1=named notes, -1=unknown)
    pub piano_roll_mode: i32,
}

impl Default for MouseContext {
    fn default() -> Self {
        Self {
            window: "unknown".to_string(),
            segment: String::new(),
            details: String::new(),
            context: Context::Main,
            window_title: String::new(),
            position: -1.0,
            midi_editor_hwnd: None,
            inline_midi: false,
            note_row: -1,
            cc_lane: -2,
            cc_lane_val: -1,
            cc_lane_id: -1,
            piano_roll_mode: -1,
        }
    }
}

// Constants are now imported from crate::input::constants

/// Get mouse position in screen coordinates
fn get_mouse_position(medium_reaper: &MediumReaper) -> (i32, i32) {
    let low_reaper = medium_reaper.low();
    let mut mouse_x: i32 = 0;
    let mut mouse_y: i32 = 0;
    unsafe {
        low_reaper.GetMousePosition(&mut mouse_x, &mut mouse_y);
    }
    (mouse_x, mouse_y)
}

/// Get the window under the mouse cursor
fn get_window_under_mouse(mouse_x: i32, mouse_y: i32) -> HWND {
    let mouse_point = POINT {
        x: mouse_x,
        y: mouse_y,
    };
    unsafe { Swell::get().WindowFromPoint(mouse_point) }
}

/// Check if an HWND is a MIDI editor window by walking up the parent chain
/// Returns (cursor_segment, midi_editor_hwnd, subview_hwnd)
/// Based on BR_MouseInfo::IsHwndMidiEditor
fn is_hwnd_midi_editor(
    hwnd: HWND,
    medium_reaper: &MediumReaper,
) -> Option<(i32, HWND, Option<HWND>)> {
    // Check if this window itself is a MIDI editor
    let mode = unsafe { medium_reaper.low().MIDIEditor_GetMode(hwnd) };

    if mode != -1 {
        // This is a MIDI editor window
        return Some((MIDI_WND_UNKNOWN, hwnd, None));
    }

    // Walk up the parent chain to find a MIDI editor parent
    let mut current = hwnd;
    while !current.is_null() {
        let parent = if let Some(window) = Window::new(current) {
            window
                .parent()
                .map(|w| w.raw_hwnd().as_ptr())
                .unwrap_or(std::ptr::null_mut())
        } else {
            std::ptr::null_mut()
        };

        if parent.is_null() {
            break;
        }

        // Check if parent is a MIDI editor
        let parent_mode = unsafe { medium_reaper.low().MIDIEditor_GetMode(parent) };

        if parent_mode != -1 {
            // Found MIDI editor parent, current is the subview
            // Check if subview is GetNotesView or GetPianoView
            if let Some(_notes_view) = reaper_windows::get_notes_view(parent, medium_reaper)
                && current == _notes_view
            {
                return Some((MIDI_WND_NOTEVIEW, parent, Some(current)));
            }
            if let Some(_piano_view) = reaper_windows::get_piano_view(parent, medium_reaper)
                && current == _piano_view
            {
                return Some((MIDI_WND_KEYBOARD, parent, Some(current)));
            }
            // Default to unknown if we can't determine
            return Some((MIDI_WND_UNKNOWN, parent, Some(current)));
        }

        current = parent;
    }

    None
}

/// Get arrange window HWND
/// TODO: This would ideally use GetArrangeWnd() from SWS, but we'll try to find it via window enumeration
fn get_arrange_wnd(medium_reaper: &MediumReaper) -> Option<HWND> {
    // Try to get it from the main window's children
    // This is a simplified approach - the real GetArrangeWnd() from SWS is more sophisticated
    let main_hwnd = medium_reaper.get_main_hwnd();
    if let Some(_main_window) = Window::new(main_hwnd.as_ptr()) {
        // The arrange view is typically a child window
        // We'll use the main window as a fallback
        return Some(main_hwnd.as_ptr());
    }
    None
}

// Use utils::get_ruler_wnd and utils::get_transport_wnd instead

/// Check if point is in arrange view
/// Based on BR_MouseUtil::IsPointInArrange
fn is_point_in_arrange(
    p: POINT,
    check_point_visibility: bool,
    medium_reaper: &MediumReaper,
) -> (bool, Option<HWND>) {
    if let Some(arrange_hwnd) = get_arrange_wnd(medium_reaper) {
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        unsafe {
            Swell::get().GetWindowRect(arrange_hwnd, &mut rect);
        }

        // Adjust for scrollbars
        rect.right -= SCROLLBAR_W + 1;
        rect.bottom -= SCROLLBAR_W + 1;

        let point_in_arrange =
            p.x >= rect.left && p.x <= rect.right && p.y >= rect.top && p.y <= rect.bottom;

        if check_point_visibility {
            if point_in_arrange {
                let hwnd_pt = get_window_under_mouse(p.x, p.y);
                if arrange_hwnd == hwnd_pt {
                    return (true, Some(hwnd_pt));
                } else {
                    return (false, Some(hwnd_pt));
                }
            } else {
                let hwnd_pt = get_window_under_mouse(p.x, p.y);
                return (false, Some(hwnd_pt));
            }
        } else {
            let hwnd_pt = get_window_under_mouse(p.x, p.y);
            return (point_in_arrange, Some(hwnd_pt));
        }
    }

    (false, None)
}

/// Get position at arrange point
/// Based on BR_MouseUtil::PositionAtArrangePoint
fn position_at_arrange_point(_p: POINT, _medium_reaper: &MediumReaper) -> (f64, f64, f64, f64) {
    // TODO: Full implementation would use GetSet_ArrangeView2 and GetHZoomLevel
    // For now, return a placeholder
    (-1.0, -1.0, -1.0, 1.0) // (position, arrangeStart, arrangeEnd, hZoom)
}

/// Translate point to arrange scroll Y
/// Based on BR_MouseUtil::TranslatePointToArrangeScrollY
fn translate_point_to_arrange_scroll_y(p: POINT, medium_reaper: &MediumReaper) -> i32 {
    if let Some(arrange_hwnd) = get_arrange_wnd(medium_reaper) {
        let mut client_p = p;
        unsafe {
            Swell::get().ScreenToClient(arrange_hwnd, &mut client_p);
        }

        // TODO: Get scroll position using CF_GetScrollInfo or similar
        // For now, just return client Y
        client_p.y
    } else {
        -1
    }
}

/// Translate point to arrange display X
/// Based on BR_MouseUtil::TranslatePointToArrangeDisplayX
fn translate_point_to_arrange_display_x(p: POINT, medium_reaper: &MediumReaper) -> i32 {
    if let Some(arrange_hwnd) = get_arrange_wnd(medium_reaper) {
        let mut client_p = p;
        unsafe {
            Swell::get().ScreenToClient(arrange_hwnd, &mut client_p);
        }
        client_p.x
    } else {
        -1
    }
}

/// Get ruler lane height
/// Based on BR_MouseInfo::GetRulerLaneHeight
fn get_ruler_lane_height(ruler_h: i32, lane: i32) -> i32 {
    // lane: 0 -> regions, 1 -> markers, 2 -> tempo, 3 -> timeline
    let timeline = (ruler_h as f64 / 2.0).round() as i32;
    let markers = ((timeline as f64 / 3.0).trunc() as i32) + 1;

    match lane {
        0 => ruler_h - markers * 2 - timeline,
        1 | 2 => markers,
        3 => timeline,
        _ => 0,
    }
}

/// Determine context from mouse position (main function)
/// Ports BR_MouseInfo::GetContext
pub fn determine_mouse_context(medium_reaper: &MediumReaper, mode: DetectionMode) -> MouseContext {
    let mut mouse_context = MouseContext::default();

    // Get mouse position
    let (mouse_x, mouse_y) = get_mouse_position(medium_reaper);
    let mouse_point = POINT {
        x: mouse_x,
        y: mouse_y,
    };

    // Find window under mouse
    let mouse_window_hwnd = get_window_under_mouse(mouse_x, mouse_y);

    if mouse_window_hwnd.is_null() {
        // Fallback to keyboard focus if we can't determine mouse window
        let (context, context_name, window_title) =
            crate::input::handler::InputHandler::determine_context();
        mouse_context.context = context;
        mouse_context.window_title = window_title;
        mouse_context.window = context_name.to_lowercase().replace(" ", "_");
        return mouse_context;
    }

    // App-defined panels claim their own context — a docked panel
    // would otherwise read as Main and its wheel events would be eaten
    // by the arrange-view wheel bindings.
    if let Some(context) = crate::input::window_detection::panel_context_of(mouse_window_hwnd) {
        mouse_context.context = context;
        mouse_context.window = format!("{context:?}").to_lowercase();
        return mouse_context;
    }

    // Get arrange view info
    let (mouse_pos, _arrange_start, _arrange_end, _arrange_zoom) =
        position_at_arrange_point(mouse_point, medium_reaper);
    let _mouse_display_x = translate_point_to_arrange_display_x(mouse_point, medium_reaper);

    let mut found = false;

    // Try to get window title
    if let Some(window) = Window::new(mouse_window_hwnd)
        && let Ok(title) = window.text()
    {
        mouse_context.window_title = title.clone();
    }

    // Ruler detection
    if !found
        && (mode.all || mode.ruler)
        && let Some(ruler_hwnd) = utils::get_ruler_wnd(medium_reaper)
        && ruler_hwnd == mouse_window_hwnd
    {
        mouse_context.window = "ruler".to_string();

        let mut ruler_p = mouse_point;
        unsafe {
            Swell::get().ScreenToClient(ruler_hwnd, &mut ruler_p);
        }

        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        unsafe {
            Swell::get().GetClientRect(ruler_hwnd, &mut rect);
        }

        let ruler_h = rect.bottom - rect.top;
        let mut limit_l;
        let mut limit_h = 0;

        for i in 0..4 {
            limit_l = limit_h;
            limit_h += get_ruler_lane_height(ruler_h, i);

            mouse_context.segment = match i {
                0 => "region_lane".to_string(),
                1 => "marker_lane".to_string(),
                2 => "tempo_lane".to_string(),
                3 => "timeline".to_string(),
                _ => String::new(),
            };

            if ruler_p.y >= limit_l && ruler_p.y < limit_h {
                break;
            }
        }

        mouse_context.position = mouse_pos;
        found = true;
    }

    // Transport detection
    if !found
        && (mode.all || mode.transport)
        && let Some(transport_hwnd) = utils::get_transport_wnd(medium_reaper)
        && transport_hwnd == mouse_window_hwnd
    {
        // Check if mouse is over time status (child of transport)
        if let Some(window) = Window::new(mouse_window_hwnd)
            && let Some(parent) = window.parent()
            && parent.raw_hwnd().as_ptr() == transport_hwnd
        {
            mouse_context.window = "transport".to_string();
            found = true;
        }
        if !found && transport_hwnd == mouse_window_hwnd {
            mouse_context.window = "transport".to_string();
            found = true;
        }
    }

    // MIDI editor detection
    if !found
        && (mode.all || mode.midi_editor)
        && let Some((cursor_segment, midi_editor_hwnd, _subview)) =
            is_hwnd_midi_editor(mouse_window_hwnd, medium_reaper)
    {
        mouse_context.window = "midi_editor".to_string();
        mouse_context.midi_editor_hwnd = Some(midi_editor_hwnd);

        // Check MIDI editor mode
        let midi_mode = unsafe { medium_reaper.low().MIDIEditor_GetMode(midi_editor_hwnd) };

        match midi_mode {
            0 => {
                mouse_context.context = Context::Midi;
                mouse_context.segment = match cursor_segment {
                    MIDI_WND_NOTEVIEW => "notes".to_string(),
                    MIDI_WND_KEYBOARD => "piano".to_string(),
                    _ => "unknown".to_string(),
                };
            }
            1 => {
                mouse_context.context = Context::MidiEventListEditor;
                mouse_context.segment = "unknown".to_string();
            }
            _ => {
                mouse_context.context = Context::Midi;
                mouse_context.segment = "unknown".to_string();
            }
        }

        // TODO: Full MIDI editor context detection (ruler, CC lanes, note rows, etc.)
        // This would require BR_MidiEditor class and more SWS functions

        found = true;
    }

    // Arrange view detection
    if !found
        && (mode.all || mode.arrange || mode.midi_inline)
        && let Some(arrange_hwnd) = utils::get_arrange_wnd(medium_reaper)
    {
        let (in_arrange, _) = is_point_in_arrange(mouse_point, false, medium_reaper);
        if arrange_hwnd == mouse_window_hwnd && in_arrange {
            mouse_context.window = "arrange".to_string();
            mouse_context.position = mouse_pos;

            let mouse_y = translate_point_to_arrange_scroll_y(mouse_point, medium_reaper);

            // Item detection using GetItemFromPoint
            let mut take_at_mouse: *mut MediaItem_Take = std::ptr::null_mut();
            let item_at_mouse = unsafe {
                medium_reaper.low().GetItemFromPoint(
                    mouse_x,
                    mouse_y,
                    true, // allow_locked
                    &mut take_at_mouse,
                )
            };

            if !item_at_mouse.is_null() {
                mouse_context.details = "item".to_string();

                // Check if it's inline MIDI
                if !take_at_mouse.is_null()
                    && midi_utils::is_open_in_inline_editor(take_at_mouse, medium_reaper)
                {
                    mouse_context.window = "midi_editor".to_string();
                    mouse_context.segment = "inline".to_string();
                    mouse_context.context = Context::MidiInlineEditor;
                    mouse_context.inline_midi = true;

                    // Get take height for inline MIDI detection
                    let mut take_offset = 0;
                    let _take_height =
                        utils::get_take_height(take_at_mouse, &mut take_offset, medium_reaper);

                    // TODO: Full inline MIDI context detection (CC lanes, note rows, etc.)
                    // This would require more MIDI editor state
                }
            } else {
                mouse_context.details = "empty".to_string();
            }

            // TODO: GetTrackOrEnvelopeFromY - would need track/envelope detection
            // TODO: Stretch marker detection
            // TODO: Envelope point/segment detection

            if mouse_context.segment.is_empty() {
                mouse_context.segment = "track".to_string(); // Default
            }

            found = true;
        }
    }

    // TCP/MCP detection
    if !found && (mode.all || mode.mcp_tcp) {
        // TODO: HwndToTrack and HwndToEnvelope from SWS
        // For now, we'll use window title matching as fallback
        let title_lower = mouse_context.window_title.to_lowercase();
        if title_lower.contains("track") || title_lower.contains("mixer") {
            mouse_context.window = if title_lower.contains("mixer") {
                "mcp".to_string()
            } else {
                "tcp".to_string()
            };
            mouse_context.segment = "track".to_string();
            found = true;
        }
    }

    // Window title-based fallbacks
    if !found {
        let title_lower = mouse_context.window_title.to_lowercase();

        if title_lower.contains("media explorer") || title_lower.contains("mediaexplorer") {
            mouse_context.window = "media_explorer".to_string();
            mouse_context.context = Context::MediaExplorer;
            found = true;
        } else if title_lower.contains("crossfade") && title_lower.contains("editor") {
            mouse_context.window = "crossfade_editor".to_string();
            mouse_context.context = Context::CrossfadeEditor;
            found = true;
        } else if (title_lower.contains("inline") || title_lower.contains("midi inline"))
            && (title_lower.contains("midi") || title_lower.contains("editor"))
        {
            mouse_context.window = "midi_editor".to_string();
            mouse_context.segment = "inline".to_string();
            mouse_context.context = Context::MidiInlineEditor;
            mouse_context.inline_midi = true;
            found = true;
        }
    }

    // Set context based on window if not already set
    if mouse_context.context == Context::Main && !found {
        mouse_context.context = match mouse_context.window.as_str() {
            "midi_editor" => Context::Midi,
            "media_explorer" => Context::MediaExplorer,
            "crossfade_editor" => Context::CrossfadeEditor,
            _ => Context::Main,
        };
    }

    mouse_context
}

/// Get mouse context and convert to our Context enum
/// This is the main function to use for wheel events
pub fn get_context_from_mouse_position(medium_reaper: &MediumReaper) -> (Context, String, String) {
    let mode = DetectionMode::minimal(); // Use minimal mode for performance
    let mouse_context = determine_mouse_context(medium_reaper, mode);
    let context_name = match mouse_context.context {
        Context::Main => "Main",
        Context::Midi => "MIDI Editor",
        Context::MidiEventListEditor => "MIDI Event List Editor",
        Context::MidiInlineEditor => "MIDI Inline Editor",
        Context::MediaExplorer => "Media Explorer",
        Context::CrossfadeEditor => "Crossfade Editor",
        Context::ExpressionEditor => "Expression Editor",
        Context::Global => "Global",
    }
    .to_string();
    (
        mouse_context.context,
        context_name,
        mouse_context.window_title,
    )
}

/// Get mouse cursor context (BR_GetMouseCursorContext equivalent)
/// Returns (window, segment, details) as strings matching SWS BR_GetMouseCursorContext format
pub fn get_mouse_cursor_context(medium_reaper: &MediumReaper) -> (String, String, String) {
    let mode = DetectionMode::all(); // Use full mode to get all details
    let mouse_context = determine_mouse_context(medium_reaper, mode);

    // Return window, segment, details matching BR_GetMouseCursorContext format
    (
        mouse_context.window.clone(),
        mouse_context.segment.clone(),
        mouse_context.details.clone(),
    )
}

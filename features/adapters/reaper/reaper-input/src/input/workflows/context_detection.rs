//! Mouse Modifier Context Detection
//!
//! Provides accurate detection of REAPER's mouse modifier contexts (MM_CTX_*).
//! This module implements the SWS BR_MouseInfo pattern for detecting
//! what element the mouse cursor is over in REAPER.
//!
//! ## Mouse Modifier Contexts
//!
//! - `MM_CTX_ITEM` - Mouse is over item body (not edge, not lower half)
//! - `MM_CTX_ITEMEDGE` - Mouse is on item left/right edge (for trimming)
//! - `MM_CTX_ITEMLOWER` - Mouse is in lower half of item (for slip editing)
//! - `MM_CTX_ITEMFADE` - Mouse is over fade handle
//! - `MM_CTX_TRACK` - Mouse is on track but not over item
//! - `MM_CTX_RULER` - Mouse is on timeline ruler
//! - `MM_CTX_ENVELOPE` - Mouse is over envelope lane
//!
//! Detection uses REAPER's `I_LASTY`/`I_LASTH` for item screen bounds.

use crate::input::constants::{
    AI_EDGE_THRESHOLD_PX, EDGE_THRESHOLD_PX, ENV_HIT_POINT, ENV_HIT_POINT_LEFT,
    FADE_HANDLE_HEIGHT_PX, LOWER_HALF_THRESHOLD, MIDI_NOTE_EDGE_HIT, MIDI_RULER_H,
};
use reaper_high::Reaper;
use reaper_hwnd::dialogs::midi_editor::MidiEditor;
use reaper_low::raw::MediaItem;
use std::sync::atomic::{AtomicBool, Ordering};

// region: --- Debug Toggle

static DEBUG_MOUSE_CONTEXT: AtomicBool = AtomicBool::new(false);

/// Toggle debug mouse context logging
pub fn toggle_debug_mouse_context() -> bool {
    let new_state = !DEBUG_MOUSE_CONTEXT.load(Ordering::Relaxed);
    DEBUG_MOUSE_CONTEXT.store(new_state, Ordering::Relaxed);
    new_state
}

/// Check if debug mouse context logging is enabled
pub fn is_debug_mouse_context_enabled() -> bool {
    DEBUG_MOUSE_CONTEXT.load(Ordering::Relaxed)
}

// endregion: --- Debug Toggle

// region: --- Types

/// Granular mouse modifier context matching REAPER's MM_CTX_* system
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseModifierContext {
    // === Media Item Contexts ===
    /// Over item body (not edge, fade, or lower half)
    Item,
    /// On item left edge (for left-trim)
    ItemLeftEdge,
    /// On item right edge (for right-trim)
    ItemRightEdge,
    /// In lower half of item (for slip editing)
    ItemLower,
    /// Over fade-in handle (top-left corner)
    ItemFadeIn,
    /// Over fade-out handle (top-right corner)
    ItemFadeOut,
    /// In crossfade area between items
    Crossfade,
    /// Over item stretch marker
    ItemStretchMarker,

    // === Track Contexts ===
    /// On track but not over item
    Track,
    /// In TCP (Track Control Panel)
    Tcp,
    /// In MCP (Mixer Control Panel)
    Mcp,

    // === Ruler/Timeline Contexts ===
    /// On timeline ruler
    Ruler,
    /// On region lane
    RegionLane,
    /// On marker lane
    MarkerLane,
    /// On tempo lane
    TempoLane,
    /// Over edit cursor handle
    CursorHandle,

    // === Project Marker/Region Contexts ===
    /// Over region body (not edge)
    Region,
    /// Over region/marker edge
    RegionMarkerEdge,
    /// Over tempo marker
    TempoMarker,

    // === Envelope Contexts ===
    /// Over envelope lane (empty area)
    Envelope,
    /// Over envelope point
    EnvelopePoint,
    /// Over envelope segment (line between points)
    EnvelopeSegment,

    // === Automation Item Contexts ===
    /// Over automation item body
    AutomationItem,
    /// Over automation item left edge
    AutomationItemLeftEdge,
    /// Over automation item right edge
    AutomationItemRightEdge,

    // === Razor Edit Contexts ===
    /// Over razor edit area
    RazorEdit,
    /// Over razor edit edge
    RazorEditEdge,
    /// Over razor edit envelope area
    RazorEditEnvelope,

    // === Fixed Lane Contexts ===
    /// Over fixed lane tab/header
    FixedLaneTab,
    /// Over fixed lane comp area
    LinkedLane,

    // === MIDI Editor Contexts ===
    /// In MIDI editor notes area (empty)
    MidiNotes,
    /// Over MIDI note
    MidiNote,
    /// Over MIDI note edge
    MidiNoteEdge,
    /// In MIDI editor piano roll (keyboard view)
    MidiPiano,
    /// In MIDI editor CC lane (empty)
    MidiCCLane,
    /// Over MIDI CC event
    MidiCCEvent,
    /// Over MIDI CC segment
    MidiCCSegment,
    /// In MIDI editor CC selector (lane header)
    MidiCCSelector,
    /// In MIDI editor ruler
    MidiRuler,
    /// Over MIDI loop end pointer
    MidiEndPointer,
    /// In MIDI marker/region lanes
    MidiMarkerLanes,

    /// Unknown/undetected context
    Unknown,
}

impl std::fmt::Display for MouseModifierContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // Media Item
            Self::Item => write!(f, "MM_CTX_ITEM"),
            Self::ItemLeftEdge => write!(f, "MM_CTX_ITEMEDGE_L"),
            Self::ItemRightEdge => write!(f, "MM_CTX_ITEMEDGE_R"),
            Self::ItemLower => write!(f, "MM_CTX_ITEMLOWER"),
            Self::ItemFadeIn => write!(f, "MM_CTX_ITEMFADE_IN"),
            Self::ItemFadeOut => write!(f, "MM_CTX_ITEMFADE_OUT"),
            Self::Crossfade => write!(f, "MM_CTX_CROSSFADE"),
            Self::ItemStretchMarker => write!(f, "MM_CTX_ITEMSTRETCHMARKER"),
            // Track
            Self::Track => write!(f, "MM_CTX_TRACK"),
            Self::Tcp => write!(f, "MM_CTX_TCP"),
            Self::Mcp => write!(f, "MM_CTX_MCP"),
            // Ruler/Timeline
            Self::Ruler => write!(f, "MM_CTX_RULER"),
            Self::RegionLane => write!(f, "MM_CTX_MARKERLANES"),
            Self::MarkerLane => write!(f, "MM_CTX_MARKERLANES"),
            Self::TempoLane => write!(f, "MM_CTX_TEMPOLANE"),
            Self::CursorHandle => write!(f, "MM_CTX_CURSORHANDLE"),
            // Project Marker/Region
            Self::Region => write!(f, "MM_CTX_REGION"),
            Self::RegionMarkerEdge => write!(f, "MM_CTX_MARKER_REGIONEDGE"),
            Self::TempoMarker => write!(f, "MM_CTX_TEMPOMARKER"),
            // Envelope
            Self::Envelope => write!(f, "MM_CTX_ENVLANE"),
            Self::EnvelopePoint => write!(f, "MM_CTX_ENVPT"),
            Self::EnvelopeSegment => write!(f, "MM_CTX_ENVSEG"),
            // Automation Item
            Self::AutomationItem => write!(f, "MM_CTX_POOLEDENV"),
            Self::AutomationItemLeftEdge => write!(f, "MM_CTX_POOLEDENVEDGE_L"),
            Self::AutomationItemRightEdge => write!(f, "MM_CTX_POOLEDENVEDGE_R"),
            // Razor Edit
            Self::RazorEdit => write!(f, "MM_CTX_AREASEL"),
            Self::RazorEditEdge => write!(f, "MM_CTX_AREASEL_EDGE"),
            Self::RazorEditEnvelope => write!(f, "MM_CTX_AREASEL_ENV"),
            // Fixed Lane
            Self::FixedLaneTab => write!(f, "MM_CTX_FIXEDLANETAB"),
            Self::LinkedLane => write!(f, "MM_CTX_LINKEDLANE"),
            // MIDI Editor
            Self::MidiNotes => write!(f, "MM_CTX_MIDI_NOTES"),
            Self::MidiNote => write!(f, "MM_CTX_MIDI_NOTE"),
            Self::MidiNoteEdge => write!(f, "MM_CTX_MIDI_NOTEEDGE"),
            Self::MidiPiano => write!(f, "MM_CTX_MIDI_PIANOROLL"),
            Self::MidiCCLane => write!(f, "MM_CTX_MIDI_CCLANE"),
            Self::MidiCCEvent => write!(f, "MM_CTX_MIDI_CCEVT"),
            Self::MidiCCSegment => write!(f, "MM_CTX_MIDI_CCSEG"),
            Self::MidiCCSelector => write!(f, "MM_CTX_MIDI_CCSELECTOR"),
            Self::MidiRuler => write!(f, "MM_CTX_MIDI_RULER"),
            Self::MidiEndPointer => write!(f, "MM_CTX_MIDI_ENDPTR"),
            Self::MidiMarkerLanes => write!(f, "MM_CTX_MIDI_MARKERLANES"),
            // Unknown
            Self::Unknown => write!(f, "MM_CTX_UNKNOWN"),
        }
    }
}

/// Detailed info about item under mouse
#[derive(Debug, Clone)]
pub struct ItemHitInfo {
    /// The item pointer
    pub item: *mut MediaItem,
    /// Item left edge in screen coordinates (relative to arrange)
    pub screen_left: i32,
    /// Item right edge in screen coordinates
    pub screen_right: i32,
    /// Item top in screen coordinates (relative to arrange, includes scroll)
    pub screen_top: i32,
    /// Item bottom in screen coordinates
    pub screen_bottom: i32,
    /// Item height
    pub height: i32,
    /// Mouse X relative to item left edge
    pub rel_x: i32,
    /// Mouse Y relative to item top
    pub rel_y: i32,
    /// Fade in length in pixels
    pub fade_in_px: i32,
    /// Fade out length in pixels
    pub fade_out_px: i32,
}

/// Detailed result from context detection
#[derive(Debug, Clone)]
pub struct MouseContextResult {
    /// The detected context
    pub context: MouseModifierContext,
    /// Item hit info (if over an item)
    pub item_info: Option<ItemHitInfo>,
    /// Mouse position (screen coordinates)
    pub mouse_x: i32,
    pub mouse_y: i32,
    /// Mouse position relative to arrange view
    pub arrange_x: i32,
    pub arrange_y: i32,
    /// Additional details string
    pub details: String,
}

impl Default for MouseContextResult {
    fn default() -> Self {
        Self {
            context: MouseModifierContext::Unknown,
            item_info: None,
            mouse_x: 0,
            mouse_y: 0,
            arrange_x: 0,
            arrange_y: 0,
            details: String::new(),
        }
    }
}

// endregion: --- Types

// region: --- Main Detection

/// Detect the precise mouse modifier context at screen coordinates
/// This is the main function for granular context detection.
/// Follows SWS BR_MouseInfo::GetContext() pattern using WindowFromPoint.
pub fn detect_context_at_point(mouse_x: i32, mouse_y: i32) -> MouseContextResult {
    use crate::input::reaper_windows;
    use reaper_low::Swell;

    let reaper = Reaper::get();
    let medium = reaper.medium_reaper();
    let low = medium.low();
    let swell = Swell::get();

    let mut result = MouseContextResult {
        context: MouseModifierContext::Unknown,
        item_info: None,
        mouse_x,
        mouse_y,
        arrange_x: 0,
        arrange_y: 0,
        details: String::new(),
    };

    // Get window under mouse cursor (SWS approach)
    let pt = reaper_low::raw::POINT {
        x: mouse_x,
        y: mouse_y,
    };
    let hwnd_under_mouse = unsafe { swell.WindowFromPoint(pt) };

    if hwnd_under_mouse.is_null() {
        result.details = "No window under mouse".to_string();
        return result;
    }

    // Get key windows for comparison
    let _main_hwnd = medium.get_main_hwnd();
    let arrange_hwnd = reaper_windows::get_arrange_wnd(medium);
    let ruler_hwnd = reaper_windows::get_ruler_wnd(medium);

    // === Check Ruler ===
    if let Some(ruler) = ruler_hwnd
        && hwnd_under_mouse == ruler
    {
        return detect_ruler_context(mouse_x, mouse_y, ruler, medium, swell);
    }

    // === Check Transport ===
    if let Some(transport) = reaper_windows::get_transport_wnd(medium) {
        let transport_parent = unsafe { swell.GetParent(hwnd_under_mouse) };
        if hwnd_under_mouse == transport || transport_parent == transport {
            result.context = MouseModifierContext::Unknown; // No specific transport context
            result.details = "window: transport".to_string();
            return result;
        }
    }

    // === Check TCP (Track Control Panel) ===
    let (tcp_hwnd, tcp_is_container) = reaper_windows::get_tcp_wnd(medium);
    if let Some(tcp) = tcp_hwnd {
        if let Some((_track, track_ctx)) =
            check_tcp_context(hwnd_under_mouse, tcp, tcp_is_container, &pt, medium, swell)
        {
            if track_ctx.is_spacer {
                result.context = MouseModifierContext::Track;
                result.details = "window: tcp, segment: track, details: spacer".to_string();
            } else {
                result.context = MouseModifierContext::Tcp;
                result.details = "window: tcp, segment: track".to_string();
            }
            return result;
        }
        // Check if over TCP but not a track (empty area or envelope)
        if hwnd_under_mouse == tcp {
            // Check for envelope
            if let Some(_env) = reaper_windows::hwnd_to_envelope(hwnd_under_mouse, pt, medium) {
                result.context = MouseModifierContext::Envelope;
                result.details = "window: tcp, segment: envelope".to_string();
                return result;
            }
            result.context = MouseModifierContext::Tcp;
            result.details = "window: tcp, segment: empty".to_string();
            return result;
        }
    }

    // === Check MCP (Mixer Control Panel) ===
    let (mcp_hwnd, _mcp_is_container) = reaper_windows::get_mcp_wnd(medium);
    if let Some(mcp) = mcp_hwnd {
        let hwnd_parent = unsafe { swell.GetParent(hwnd_under_mouse) };
        if hwnd_under_mouse == mcp || hwnd_parent == mcp {
            result.context = MouseModifierContext::Mcp;
            result.details = "window: mcp, segment: track".to_string();
            return result;
        }
    }

    // === Check MIDI Editor ===
    // Check if window is a MIDI editor or child of one
    let midi_editor_mode = unsafe { low.MIDIEditor_GetMode(hwnd_under_mouse) };
    if midi_editor_mode != -1 {
        // hwnd_under_mouse IS the midi editor - check subviews
        return detect_midi_editor_context(hwnd_under_mouse, mouse_x, mouse_y, medium, swell);
    }
    // Check parent for MIDI editor
    let hwnd_parent = unsafe { swell.GetParent(hwnd_under_mouse) };
    if !hwnd_parent.is_null() {
        let parent_midi_mode = unsafe { low.MIDIEditor_GetMode(hwnd_parent) };
        if parent_midi_mode != -1 {
            return detect_midi_editor_context(hwnd_parent, mouse_x, mouse_y, medium, swell);
        }
    }

    // === Check Arrange View ===
    if let Some(arrange) = arrange_hwnd
        && hwnd_under_mouse == arrange
    {
        return detect_arrange_context(mouse_x, mouse_y, arrange, medium, swell);
    }

    // Unknown window
    result.details = format!("Unknown window (hwnd: {:?})", hwnd_under_mouse);
    result
}

/// Detect the likely mouse modifier context based on mouse position
/// Returns (context_name, details) for debugging
///
/// This function uses the comprehensive detection system
pub fn detect_mouse_modifier_context(mouse_x: i32, mouse_y: i32) -> (String, String) {
    let result = detect_context_at_point(mouse_x, mouse_y);
    (result.context.to_string(), result.details)
}

// endregion: --- Main Detection

// region: --- Ruler Detection

/// Detect context within the ruler
fn detect_ruler_context(
    mouse_x: i32,
    mouse_y: i32,
    ruler_hwnd: reaper_low::raw::HWND,
    medium: &reaper_medium::Reaper,
    swell: &reaper_low::Swell,
) -> MouseContextResult {
    let low = medium.low();

    let mut result = MouseContextResult {
        context: MouseModifierContext::Ruler,
        item_info: None,
        mouse_x,
        mouse_y,
        arrange_x: 0,
        arrange_y: 0,
        details: String::new(),
    };

    let mut ruler_rect = reaper_low::raw::RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    unsafe {
        swell.GetClientRect(ruler_hwnd, &mut ruler_rect);
    }

    let mut pt_ruler = reaper_low::raw::POINT {
        x: mouse_x,
        y: mouse_y,
    };
    unsafe {
        swell.ScreenToClient(ruler_hwnd, &mut pt_ruler);
    }

    let ruler_h = ruler_rect.bottom - ruler_rect.top;
    // SWS GetRulerLaneHeight logic
    let timeline_h = (ruler_h as f64 / 2.0).round() as i32;
    let marker_h = (timeline_h as f64 / 3.0).trunc() as i32 + 1;
    let region_h = ruler_h - marker_h * 2 - timeline_h;

    // Get arrange view time info for coordinate conversion
    let mut start_time: f64 = 0.0;
    let mut end_time: f64 = 0.0;
    unsafe {
        low.GetSet_ArrangeView2(
            std::ptr::null_mut(),
            false,
            0,
            0,
            &mut start_time,
            &mut end_time,
        );
    }

    let ruler_width = (ruler_rect.right - ruler_rect.left) as f64;
    let time_range = end_time - start_time;
    let ruler_zoom = if time_range > 0.0 {
        ruler_width / time_range
    } else {
        1.0
    };

    // Convert mouse X to time position
    let mouse_time = start_time + (pt_ruler.x as f64) / ruler_zoom;

    // Edge threshold in pixels
    const MARKER_EDGE_THRESHOLD: i32 = 5;

    let mut lane_bottom = 0;
    if pt_ruler.y < {
        lane_bottom += region_h;
        lane_bottom
    } {
        // In region lane - check for specific regions
        if let Some((ctx, details)) = detect_region_at_time(
            mouse_time,
            pt_ruler.x,
            ruler_zoom,
            start_time,
            low,
            MARKER_EDGE_THRESHOLD,
            true,
        ) {
            result.context = ctx;
            result.details = details;
        } else {
            result.context = MouseModifierContext::RegionLane;
            result.details = "window: ruler, segment: region_lane".to_string();
        }
    } else if pt_ruler.y < {
        lane_bottom += marker_h;
        lane_bottom
    } {
        // In marker lane - check for specific markers
        if let Some((ctx, details)) = detect_region_at_time(
            mouse_time,
            pt_ruler.x,
            ruler_zoom,
            start_time,
            low,
            MARKER_EDGE_THRESHOLD,
            false,
        ) {
            result.context = ctx;
            result.details = details;
        } else {
            result.context = MouseModifierContext::MarkerLane;
            result.details = "window: ruler, segment: marker_lane".to_string();
        }
    } else if pt_ruler.y < {
        lane_bottom += marker_h;
        lane_bottom
    } {
        // In tempo lane - check for specific tempo markers
        if let Some((ctx, details)) = detect_tempo_marker_at_time(
            mouse_time,
            pt_ruler.x,
            ruler_zoom,
            start_time,
            low,
            MARKER_EDGE_THRESHOLD,
        ) {
            result.context = ctx;
            result.details = details;
        } else {
            result.context = MouseModifierContext::TempoLane;
            result.details = "window: ruler, segment: tempo_lane".to_string();
        }
    } else {
        result.context = MouseModifierContext::Ruler;
        result.details = "window: ruler, segment: timeline".to_string();
    }

    result
}

/// Detect region or marker at a specific time position
fn detect_region_at_time(
    mouse_time: f64,
    mouse_x: i32,
    zoom: f64,
    start_time: f64,
    low: &reaper_low::Reaper,
    edge_threshold: i32,
    is_region_lane: bool,
) -> Option<(MouseModifierContext, String)> {
    let mut idx = 0;
    loop {
        let mut is_region = false;
        let mut pos: f64 = 0.0;
        let mut end: f64 = 0.0;
        let mut name_ptr: *const std::ffi::c_char = std::ptr::null();
        let mut marker_idx: i32 = 0;

        let result = unsafe {
            low.EnumProjectMarkers3(
                std::ptr::null_mut(),
                idx,
                &mut is_region,
                &mut pos,
                &mut end,
                &mut name_ptr,
                &mut marker_idx,
                std::ptr::null_mut(),
            )
        };

        if result == 0 {
            break; // No more markers
        }

        idx += 1;

        // Skip if wrong type (region vs marker)
        if is_region != is_region_lane {
            continue;
        }

        if is_region {
            // It's a region - check if mouse is within or at edge
            let region_start_x = ((pos - start_time) * zoom) as i32;
            let region_end_x = ((end - start_time) * zoom) as i32;

            // Check start edge
            if mouse_x >= region_start_x - edge_threshold
                && mouse_x <= region_start_x + edge_threshold
            {
                return Some((
                    MouseModifierContext::RegionMarkerEdge,
                    format!(
                        "window: ruler, segment: region_lane, details: region_edge_left (idx: {}, pos: {:.3})",
                        marker_idx, pos
                    ),
                ));
            }

            // Check end edge
            if mouse_x >= region_end_x - edge_threshold && mouse_x <= region_end_x + edge_threshold
            {
                return Some((
                    MouseModifierContext::RegionMarkerEdge,
                    format!(
                        "window: ruler, segment: region_lane, details: region_edge_right (idx: {}, end: {:.3})",
                        marker_idx, end
                    ),
                ));
            }

            // Check body
            if mouse_time >= pos && mouse_time <= end {
                return Some((
                    MouseModifierContext::Region,
                    format!(
                        "window: ruler, segment: region_lane, details: region (idx: {}, pos: {:.3}, end: {:.3})",
                        marker_idx, pos, end
                    ),
                ));
            }
        } else {
            // It's a marker - check if mouse is near it
            let marker_x = ((pos - start_time) * zoom) as i32;

            if mouse_x >= marker_x - edge_threshold && mouse_x <= marker_x + edge_threshold {
                return Some((
                    MouseModifierContext::RegionMarkerEdge,
                    format!(
                        "window: ruler, segment: marker_lane, details: marker (idx: {}, pos: {:.3})",
                        marker_idx, pos
                    ),
                ));
            }
        }
    }

    None
}

/// Detect tempo marker at a specific time position
fn detect_tempo_marker_at_time(
    _mouse_time: f64,
    mouse_x: i32,
    zoom: f64,
    start_time: f64,
    low: &reaper_low::Reaper,
    edge_threshold: i32,
) -> Option<(MouseModifierContext, String)> {
    // Get number of tempo markers
    let tempo_count = unsafe { low.CountTempoTimeSigMarkers(std::ptr::null_mut()) };

    for idx in 0..tempo_count {
        let mut time_pos: f64 = 0.0;
        let mut _measure: i32 = 0;
        let mut _beat: f64 = 0.0;
        let mut _bpm: f64 = 0.0;
        let mut _timesig_num: i32 = 0;
        let mut _timesig_denom: i32 = 0;
        let mut _lineartempo: bool = false;

        let success = unsafe {
            low.GetTempoTimeSigMarker(
                std::ptr::null_mut(),
                idx,
                &mut time_pos,
                &mut _measure,
                &mut _beat,
                &mut _bpm,
                &mut _timesig_num,
                &mut _timesig_denom,
                &mut _lineartempo,
            )
        };

        if !success {
            continue;
        }

        let marker_x = ((time_pos - start_time) * zoom) as i32;

        if mouse_x >= marker_x - edge_threshold && mouse_x <= marker_x + edge_threshold {
            return Some((
                MouseModifierContext::TempoMarker,
                format!(
                    "window: ruler, segment: tempo_lane, details: tempo_marker (idx: {}, pos: {:.3})",
                    idx, time_pos
                ),
            ));
        }
    }

    None
}

// endregion: --- Ruler Detection

// region: --- MIDI Editor Detection

/// Detect context within the MIDI editor
/// Detects: ruler, notes view, piano view, CC lanes, CC selector, and individual notes
fn detect_midi_editor_context(
    midi_editor: reaper_low::raw::HWND,
    mouse_x: i32,
    mouse_y: i32,
    medium: &reaper_medium::Reaper,
    swell: &reaper_low::Swell,
) -> MouseContextResult {
    use crate::input::reaper_windows;

    let low = medium.low();

    let mut result = MouseContextResult {
        context: MouseModifierContext::Unknown,
        item_info: None,
        mouse_x,
        mouse_y,
        arrange_x: 0,
        arrange_y: 0,
        details: String::new(),
    };

    // Get window under mouse
    let pt = reaper_low::raw::POINT {
        x: mouse_x,
        y: mouse_y,
    };
    let hwnd_under_mouse = unsafe { swell.WindowFromPoint(pt) };

    // Check if we're over the piano (keyboard) view
    if let Some(piano_view) = reaper_windows::get_piano_view(midi_editor, medium)
        && hwnd_under_mouse == piano_view
    {
        result.context = MouseModifierContext::MidiPiano;
        result.details = "window: midi_editor, segment: piano".to_string();
        return result;
    }

    // Check if we're over the notes view
    if let Some(notes_view) = reaper_windows::get_notes_view(midi_editor, medium)
        && hwnd_under_mouse == notes_view
    {
        // Convert mouse to client coordinates relative to notes view
        let mut pt_client = reaper_low::raw::POINT {
            x: mouse_x,
            y: mouse_y,
        };
        unsafe {
            swell.ScreenToClient(notes_view, &mut pt_client);
        }

        // Get client rect for notes view
        let mut client_rect = reaper_low::raw::RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        unsafe {
            swell.GetClientRect(notes_view, &mut client_rect);
        }

        let notes_height = client_rect.bottom - client_rect.top;
        let notes_width = client_rect.right - client_rect.left;

        // Scale ruler height by DPI if available (approximation)
        // On macOS, we don't have easy DPI access, so use base value
        let ruler_h = MIDI_RULER_H;

        // === Check for MIDI Ruler (top area) ===
        if pt_client.y < ruler_h {
            result.context = MouseModifierContext::MidiRuler;
            result.details = format!(
                "window: midi_editor, segment: ruler (y: {}, ruler_h: {})",
                pt_client.y, ruler_h
            );
            return result;
        }

        // Try to get last clicked CC lane to determine if CC lanes are visible
        // last_clicked_cc_lane returns -2 if no lane, or lane number if there's a visible lane
        let last_cc_lane =
            unsafe { low.MIDIEditor_GetSetting_int(midi_editor, c"last_clicked_cc_lane".as_ptr()) };

        // If CC lanes exist (last_clicked != -2), check if mouse is in lower portion
        // SWS uses complex chunk parsing to get exact CC heights, we use a simpler heuristic
        // The CC lanes are typically at the bottom ~30% of the notes view
        if last_cc_lane != -2 {
            let cc_area_estimate = (notes_height as f64 * 0.3) as i32;
            if pt_client.y > notes_height - cc_area_estimate {
                // We're in CC lane area - try to detect specific CC events
                let take = unsafe { low.MIDIEditor_GetTake(midi_editor) };
                if !take.is_null()
                    && let Some((cc_ctx, cc_details)) = detect_midi_cc_at_point(
                        take,
                        pt_client.x,
                        pt_client.y,
                        notes_width,
                        notes_height,
                        cc_area_estimate,
                        midi_editor,
                        low,
                    )
                {
                    result.context = cc_ctx;
                    result.details = cc_details;
                    return result;
                }

                // No specific CC event - just CC lane
                result.context = MouseModifierContext::MidiCCLane;
                result.details = format!(
                    "window: midi_editor, segment: cc_lane (y: {}, cc_start: ~{})",
                    pt_client.y,
                    notes_height - cc_area_estimate
                );
                return result;
            }
        }

        // === Check for MIDI Notes ===
        // We're in the notes area - try to detect specific notes
        let take = unsafe { low.MIDIEditor_GetTake(midi_editor) };
        if !take.is_null()
            && let Some((note_ctx, note_details)) = detect_midi_note_at_point(
                take,
                pt_client.x,
                pt_client.y,
                notes_width,
                notes_height,
                ruler_h,
                midi_editor,
                low,
            )
        {
            result.context = note_ctx;
            result.details = note_details;
            return result;
        }

        // Not over a specific note - we're in notes area
        result.context = MouseModifierContext::MidiNotes;
        result.details = format!(
            "window: midi_editor, segment: notes (y: {}, height: {})",
            pt_client.y, notes_height
        );
        return result;
    }

    // Check for CC selector (lane header) - to the left of the notes view.
    // SWS detects this via control ID WndControlIDs::midi_ccLaneSelector.
    let cc_selector =
        unsafe { swell.GetDlgItem(midi_editor, MidiEditor::CC_LANE_SELECTOR.raw() as i32) };
    if !cc_selector.is_null() && hwnd_under_mouse == cc_selector {
        result.context = MouseModifierContext::MidiCCSelector;
        result.details = "window: midi_editor, segment: cc_selector".to_string();
        return result;
    }

    // Default: somewhere in MIDI editor we don't specifically recognize
    result.context = MouseModifierContext::Unknown;
    result.details = "window: midi_editor, segment: unknown".to_string();
    result
}

/// Detect if mouse is over a specific MIDI note
/// Returns (context, details) if a note is found, None otherwise
fn detect_midi_note_at_point(
    take: *mut reaper_low::raw::MediaItem_Take,
    mouse_x: i32,
    _mouse_y: i32,
    view_width: i32,
    view_height: i32,
    ruler_h: i32,
    _midi_editor: reaper_low::raw::HWND,
    low: &reaper_low::Reaper,
) -> Option<(MouseModifierContext, String)> {
    // Get MIDI editor settings
    // Unfortunately, getting precise zoom/scroll values requires chunk parsing (SWS approach)
    // We'll use available API settings and heuristics

    // Get the take's source for PPQ info
    let mut note_count: i32 = 0;
    let mut cc_count: i32 = 0;
    let mut sysex_count: i32 = 0;

    unsafe {
        low.MIDI_CountEvts(take, &mut note_count, &mut cc_count, &mut sysex_count);
    }

    if note_count == 0 {
        return None;
    }

    // Get visible time range from arrange view (approximate)
    // For a more accurate approach, we'd need to parse the MIDI editor chunk
    let mut start_time: f64 = 0.0;
    let mut end_time: f64 = 0.0;
    unsafe {
        low.GetSet_ArrangeView2(
            std::ptr::null_mut(),
            false,
            0,
            0,
            &mut start_time,
            &mut end_time,
        );
    }

    // Convert view time range to PPQ
    let _start_ppq = unsafe { low.MIDI_GetPPQPosFromProjTime(take, start_time) };
    let _end_ppq = unsafe { low.MIDI_GetPPQPosFromProjTime(take, end_time) };

    let time_range = end_time - start_time;
    if time_range <= 0.0 {
        return None;
    }

    // Calculate horizontal zoom (pixels per second)
    let h_zoom = view_width as f64 / time_range;

    // Calculate approximate vertical zoom
    // The notes area is from ruler_h to view_height
    // Standard MIDI range is 128 notes, but visible range is typically smaller
    let notes_area_height = view_height - ruler_h;

    // Assume we're showing ~24-48 notes vertically (2-4 octaves)
    // This is a heuristic; real value would need chunk parsing
    let visible_notes = 36; // ~3 octaves as default assumption
    let _v_zoom = notes_area_height as f64 / visible_notes as f64;

    // Mouse position in time
    let mouse_time = start_time + (mouse_x as f64) / h_zoom;
    let _mouse_ppq = unsafe { low.MIDI_GetPPQPosFromProjTime(take, mouse_time) };

    // Check each note for hit
    for note_idx in 0..note_count {
        let mut selected = false;
        let mut muted = false;
        let mut start_ppq_pos: f64 = 0.0;
        let mut end_ppq_pos: f64 = 0.0;
        let mut channel: i32 = 0;
        let mut pitch: i32 = 0;
        let mut velocity: i32 = 0;

        let success = unsafe {
            low.MIDI_GetNote(
                take,
                note_idx,
                &mut selected,
                &mut muted,
                &mut start_ppq_pos,
                &mut end_ppq_pos,
                &mut channel,
                &mut pitch,
                &mut velocity,
            )
        };

        if !success {
            continue;
        }

        // Convert PPQ to project time
        let note_start_time = unsafe { low.MIDI_GetProjTimeFromPPQPos(take, start_ppq_pos) };
        let note_end_time = unsafe { low.MIDI_GetProjTimeFromPPQPos(take, end_ppq_pos) };

        // Convert to screen coordinates
        let note_start_x = ((note_start_time - start_time) * h_zoom) as i32;
        let note_end_x = ((note_end_time - start_time) * h_zoom) as i32;

        // Skip notes outside horizontal view
        if note_end_x < 0 || note_start_x > view_width {
            continue;
        }

        // Check horizontal hit (with edge detection)
        if mouse_x >= note_start_x - MIDI_NOTE_EDGE_HIT
            && mouse_x <= note_end_x + MIDI_NOTE_EDGE_HIT
        {
            // Check if on left edge
            if mouse_x >= note_start_x - MIDI_NOTE_EDGE_HIT
                && mouse_x <= note_start_x + MIDI_NOTE_EDGE_HIT
            {
                return Some((
                    MouseModifierContext::MidiNoteEdge,
                    format!(
                        "window: midi_editor, segment: notes, details: note_edge_left (idx: {}, pitch: {}, start: {:.3})",
                        note_idx, pitch, note_start_time
                    ),
                ));
            }

            // Check if on right edge
            if mouse_x >= note_end_x - MIDI_NOTE_EDGE_HIT
                && mouse_x <= note_end_x + MIDI_NOTE_EDGE_HIT
            {
                return Some((
                    MouseModifierContext::MidiNoteEdge,
                    format!(
                        "window: midi_editor, segment: notes, details: note_edge_right (idx: {}, pitch: {}, end: {:.3})",
                        note_idx, pitch, note_end_time
                    ),
                ));
            }

            // In note body (horizontal match, would need Y check for precise hit)
            // For now, report as note if horizontal match
            // A more precise implementation would also check pitch/Y position
            return Some((
                MouseModifierContext::MidiNote,
                format!(
                    "window: midi_editor, segment: notes, details: note (idx: {}, pitch: {}, ch: {})",
                    note_idx, pitch, channel
                ),
            ));
        }
    }

    None
}

/// Detect if mouse is over a specific CC event
fn detect_midi_cc_at_point(
    take: *mut reaper_low::raw::MediaItem_Take,
    mouse_x: i32,
    _mouse_y: i32,
    view_width: i32,
    _view_height: i32,
    _cc_area_height: i32,
    _midi_editor: reaper_low::raw::HWND,
    low: &reaper_low::Reaper,
) -> Option<(MouseModifierContext, String)> {
    // Get CC event count
    let mut note_count: i32 = 0;
    let mut cc_count: i32 = 0;
    let mut sysex_count: i32 = 0;

    unsafe {
        low.MIDI_CountEvts(take, &mut note_count, &mut cc_count, &mut sysex_count);
    }

    if cc_count == 0 {
        return None;
    }

    // Get visible time range
    let mut start_time: f64 = 0.0;
    let mut end_time: f64 = 0.0;
    unsafe {
        low.GetSet_ArrangeView2(
            std::ptr::null_mut(),
            false,
            0,
            0,
            &mut start_time,
            &mut end_time,
        );
    }

    let time_range = end_time - start_time;
    if time_range <= 0.0 {
        return None;
    }

    // Calculate horizontal zoom
    let h_zoom = view_width as f64 / time_range;

    // CC hit threshold
    const CC_HIT_POINT: i32 = 5;

    // Check each CC event for hit
    for cc_idx in 0..cc_count {
        let mut selected = false;
        let mut muted = false;
        let mut ppq_pos: f64 = 0.0;
        let mut chan_msg: i32 = 0;
        let mut channel: i32 = 0;
        let mut msg2: i32 = 0; // CC number or pitch bend LSB
        let mut msg3: i32 = 0; // CC value or pitch bend MSB

        let success = unsafe {
            low.MIDI_GetCC(
                take,
                cc_idx,
                &mut selected,
                &mut muted,
                &mut ppq_pos,
                &mut chan_msg,
                &mut channel,
                &mut msg2,
                &mut msg3,
            )
        };

        if !success {
            continue;
        }

        // Convert PPQ to project time
        let cc_time = unsafe { low.MIDI_GetProjTimeFromPPQPos(take, ppq_pos) };

        // Convert to screen X
        let cc_x = ((cc_time - start_time) * h_zoom) as i32;

        // Check horizontal hit
        if mouse_x >= cc_x - CC_HIT_POINT && mouse_x <= cc_x + CC_HIT_POINT {
            return Some((
                MouseModifierContext::MidiCCEvent,
                format!(
                    "window: midi_editor, segment: cc_lane, details: cc_event (idx: {}, cc: {}, val: {}, ch: {})",
                    cc_idx, msg2, msg3, channel
                ),
            ));
        }
    }

    // Check for CC segment (between events)
    // If we're in the CC lane but not on a specific event, we're on a segment
    // For now, we don't implement segment detection (would need to find adjacent events)

    None
}

// endregion: --- MIDI Editor Detection

// region: --- Arrange Detection

/// Detect context within the arrange view
/// Follows SWS BR_MouseInfo pattern for arrange detection
fn detect_arrange_context(
    mouse_x: i32,
    mouse_y: i32,
    arrange_hwnd: reaper_low::raw::HWND,
    medium: &reaper_medium::Reaper,
    swell: &reaper_low::Swell,
) -> MouseContextResult {
    let low = medium.low();

    let mut result = MouseContextResult {
        context: MouseModifierContext::Track,
        item_info: None,
        mouse_x,
        mouse_y,
        arrange_x: 0,
        arrange_y: 0,
        details: String::new(),
    };

    // Convert to client coords
    let mut pt_client = reaper_low::raw::POINT {
        x: mouse_x,
        y: mouse_y,
    };
    unsafe {
        swell.ScreenToClient(arrange_hwnd, &mut pt_client);
    }
    result.arrange_x = pt_client.x;
    result.arrange_y = pt_client.y;

    // Get arrange view time info for coordinate conversion
    let mut start_time: f64 = 0.0;
    let mut end_time: f64 = 0.0;
    unsafe {
        low.GetSet_ArrangeView2(
            std::ptr::null_mut(),
            false,
            0,
            0,
            &mut start_time,
            &mut end_time,
        );
    }

    // Get arrange view width for zoom calculation
    let mut arrange_rect = reaper_low::raw::RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    unsafe {
        swell.GetClientRect(arrange_hwnd, &mut arrange_rect);
    }

    let arrange_width = (arrange_rect.right - arrange_rect.left) as f64;
    let time_range = end_time - start_time;
    let arrange_zoom = if time_range > 0.0 {
        arrange_width / time_range
    } else {
        1.0
    };

    // === Check for edit cursor handle ===
    let edit_cursor_pos = low.GetCursorPosition();
    let cursor_screen_x = ((edit_cursor_pos - start_time) * arrange_zoom) as i32;

    // Check if mouse is within cursor handle threshold (thin vertical line)
    const CURSOR_HANDLE_THRESHOLD: i32 = 4;
    if pt_client.x >= cursor_screen_x - CURSOR_HANDLE_THRESHOLD
        && pt_client.x <= cursor_screen_x + CURSOR_HANDLE_THRESHOLD
    {
        result.context = MouseModifierContext::CursorHandle;
        result.details = format!(
            "window: arrange, segment: cursor_handle, details: edit_cursor (time: {:.3})",
            edit_cursor_pos
        );
        return result;
    }

    // First check: Is mouse over an envelope lane (not track lane)?
    // GetTrackFromPoint returns track and puts context (0=track, 1=envelope, 2=empty) in info param
    let mut track_info: i32 = 0;
    let track_ptr = unsafe { low.GetTrackFromPoint(mouse_x, mouse_y, &mut track_info) };

    // track_info: 0=in track media lane, 1=in envelope lane, 2=in empty space
    if track_info == 1 && !track_ptr.is_null() {
        // Mouse is over an envelope lane - check for point/segment hit
        let env_result = detect_envelope_context(
            mouse_x,
            mouse_y,
            pt_client.x,
            pt_client.y,
            track_ptr,
            arrange_hwnd,
            medium,
            swell,
        );
        result.context = env_result.0;
        result.details = env_result.1;
        return result;
    }

    if track_info == 2 || track_ptr.is_null() {
        // Empty arrange area (no track under mouse)
        result.context = MouseModifierContext::Unknown;
        result.details = "window: arrange, segment: empty".to_string();
        return result;
    }

    // We're in a track lane (track_info == 0) - check for items
    let mut take_out: *mut reaper_low::raw::MediaItem_Take = std::ptr::null_mut();
    let item = unsafe { low.GetItemFromPoint(mouse_x, mouse_y, true, &mut take_out) };

    if !item.is_null() {
        // We're over an item - detect which part (edge, lower, fade, body)
        result.context =
            detect_item_context_detailed(item, pt_client.x, pt_client.y, medium, &mut result);
        let detail_str = match result.context {
            MouseModifierContext::Item => "item",
            MouseModifierContext::ItemLeftEdge | MouseModifierContext::ItemRightEdge => "item",
            MouseModifierContext::ItemLower => "item",
            MouseModifierContext::ItemFadeIn | MouseModifierContext::ItemFadeOut => "item",
            _ => "item",
        };
        result.details = format!("window: arrange, segment: track, details: {}", detail_str);
        return result;
    }

    // No item - check for razor edit areas
    // Razor edits are stored in track property P_RAZOREDITS as "start end isEnvelope, ..."
    let mouse_time = start_time + (pt_client.x as f64) / arrange_zoom;
    if let Some(razor_ctx) = detect_razor_edit(
        track_ptr,
        mouse_time,
        arrange_zoom,
        pt_client.x,
        start_time,
        low,
    ) {
        result.context = razor_ctx.0;
        result.details = razor_ctx.1;
        return result;
    }

    // No razor edit - we're over empty track area
    result.context = MouseModifierContext::Track;
    result.details = "window: arrange, segment: track, details: empty".to_string();
    result
}

/// Detect context within an envelope lane (point, segment, or just lane)
/// Follows SWS BR_MouseInfo::IsMouseOverEnvelopeLine pattern
fn detect_envelope_context(
    _mouse_x: i32,
    _mouse_y: i32,
    arrange_x: i32,
    _arrange_y: i32,
    track: *mut reaper_low::raw::MediaTrack,
    arrange_hwnd: reaper_low::raw::HWND,
    medium: &reaper_medium::Reaper,
    swell: &reaper_low::Swell,
) -> (MouseModifierContext, String) {
    let low = medium.low();

    // Get arrange view time info for conversion
    let mut start_time: f64 = 0.0;
    let mut end_time: f64 = 0.0;
    unsafe {
        low.GetSet_ArrangeView2(
            std::ptr::null_mut(),
            false,
            0,
            0,
            &mut start_time,
            &mut end_time,
        );
    }

    // Get arrange view width for zoom calculation
    let mut arrange_rect = reaper_low::raw::RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    unsafe {
        swell.GetClientRect(arrange_hwnd, &mut arrange_rect);
    }

    let arrange_width = (arrange_rect.right - arrange_rect.left) as f64;
    let time_range = end_time - start_time;
    let arrange_zoom = if time_range > 0.0 {
        arrange_width / time_range
    } else {
        1.0
    };

    // Convert mouse X to time position
    let mouse_time = start_time + (arrange_x as f64) / arrange_zoom;

    // Get envelopes on this track - find which one we're in
    let env_count = unsafe { low.CountTrackEnvelopes(track) };

    for i in 0..env_count {
        let envelope = unsafe { low.GetTrackEnvelope(track, i) };
        if envelope.is_null() {
            continue;
        }

        // Check if this envelope has its own lane (VIS property)
        // GetEnvelopeInfo_Value with "I_TCPH" gives TCP lane height (0 if no dedicated lane)
        let lane_height = unsafe { low.GetEnvelopeInfo_Value(envelope, c"I_TCPH".as_ptr()) } as i32;

        // Only check envelopes that have their own lane
        if lane_height <= 0 {
            continue;
        }

        // Get envelope lane Y offset in TCP (I_TCPY)
        // (Currently unused, but kept for future Y-coordinate hit testing)
        let _lane_y_tcp = unsafe { low.GetEnvelopeInfo_Value(envelope, c"I_TCPY".as_ptr()) } as i32;

        // For arrange view, we need to translate TCP Y to arrange Y
        // The envelope lane in arrange corresponds to the same visual position as TCP
        // However, the arrange view Y includes scroll offset
        // We use mouse_y (screen coords) to check against track bounds from GetTrackFromPoint

        // === Check for Automation Items first ===
        // Automation items have priority over raw envelope points
        let ai_count = unsafe { low.CountAutomationItems(envelope) };

        for ai_idx in 0..ai_count {
            // Get automation item position and length
            let ai_pos = unsafe {
                low.GetSetAutomationItemInfo(envelope, ai_idx, c"D_POSITION".as_ptr(), 0.0, false)
            };
            let ai_len = unsafe {
                low.GetSetAutomationItemInfo(envelope, ai_idx, c"D_LENGTH".as_ptr(), 0.0, false)
            };
            let ai_end = ai_pos + ai_len;

            // Calculate AI screen positions
            let ai_left_x = ((ai_pos - start_time) * arrange_zoom) as i32;
            let ai_right_x = ((ai_end - start_time) * arrange_zoom) as i32;

            // Check edges FIRST using pixel coordinates (edges extend beyond AI bounds)
            // Left edge - check if mouse X is within threshold of left edge
            if arrange_x >= ai_left_x - AI_EDGE_THRESHOLD_PX
                && arrange_x <= ai_left_x + AI_EDGE_THRESHOLD_PX
            {
                return (
                    MouseModifierContext::AutomationItemLeftEdge,
                    format!(
                        "window: arrange, segment: envelope, details: automation_item_edge_left (ai: {}, pos: {:.3}, mouse_x: {}, edge_x: {})",
                        ai_idx, ai_pos, arrange_x, ai_left_x
                    ),
                );
            }

            // Right edge - check if mouse X is within threshold of right edge
            if arrange_x >= ai_right_x - AI_EDGE_THRESHOLD_PX
                && arrange_x <= ai_right_x + AI_EDGE_THRESHOLD_PX
            {
                return (
                    MouseModifierContext::AutomationItemRightEdge,
                    format!(
                        "window: arrange, segment: envelope, details: automation_item_edge_right (ai: {}, end: {:.3}, mouse_x: {}, edge_x: {})",
                        ai_idx, ai_end, arrange_x, ai_right_x
                    ),
                );
            }

            // Check if mouse is inside the automation item body (not at edges)
            if arrange_x > ai_left_x + AI_EDGE_THRESHOLD_PX
                && arrange_x < ai_right_x - AI_EDGE_THRESHOLD_PX
            {
                // We're in the body of the automation item
                // Check for envelope points within this AI
                let point_count = unsafe { low.CountEnvelopePoints(envelope) };

                if point_count > 0 {
                    let nearest_point_idx =
                        unsafe { low.GetEnvelopePointByTime(envelope, mouse_time) };

                    // Check points around the nearest one
                    let search_range = 3;
                    let start_idx = (nearest_point_idx - search_range).max(0);
                    let end_idx = (nearest_point_idx + search_range).min(point_count - 1);

                    for point_idx in start_idx..=end_idx {
                        let mut point_time: f64 = 0.0;
                        let mut point_value: f64 = 0.0;
                        let mut _shape: i32 = 0;
                        let mut _tension: f64 = 0.0;
                        let mut _selected: bool = false;

                        let success = unsafe {
                            low.GetEnvelopePoint(
                                envelope,
                                point_idx,
                                &mut point_time,
                                &mut point_value,
                                &mut _shape,
                                &mut _tension,
                                &mut _selected,
                            )
                        };

                        if !success {
                            continue;
                        }

                        // Only check points within this AI's time range
                        if point_time < ai_pos || point_time > ai_end {
                            continue;
                        }

                        let point_screen_x = ((point_time - start_time) * arrange_zoom) as i32;

                        if arrange_x >= point_screen_x - ENV_HIT_POINT
                            && arrange_x <= point_screen_x + ENV_HIT_POINT_LEFT
                        {
                            return (
                                MouseModifierContext::EnvelopePoint,
                                format!(
                                    "window: arrange, segment: envelope, details: env_point (ai: {}, idx: {}, time: {:.3})",
                                    ai_idx, point_idx, point_time
                                ),
                            );
                        }
                    }
                }

                // In automation item body, not over a point
                return (
                    MouseModifierContext::AutomationItem,
                    format!(
                        "window: arrange, segment: envelope, details: automation_item (ai: {}, pos: {:.3}, len: {:.3})",
                        ai_idx, ai_pos, ai_len
                    ),
                );
            }
        }

        // === Not over automation item - check raw envelope ===
        // Check if mouse is vertically within this envelope's lane
        // Since GetTrackFromPoint already told us we're in an envelope lane,
        // we can iterate through envelope points to find hits

        // Get point count
        let point_count = unsafe { low.CountEnvelopePoints(envelope) };

        if point_count > 0 {
            // Find the point nearest to mouse time
            let nearest_point_idx = unsafe { low.GetEnvelopePointByTime(envelope, mouse_time) };

            // Check points around the nearest one
            let search_range = 3; // Check a few points in each direction
            let start_idx = (nearest_point_idx - search_range).max(0);
            let end_idx = (nearest_point_idx + search_range).min(point_count - 1);

            for point_idx in start_idx..=end_idx {
                let mut point_time: f64 = 0.0;
                let mut point_value: f64 = 0.0;
                let mut _shape: i32 = 0;
                let mut _tension: f64 = 0.0;
                let mut _selected: bool = false;

                let success = unsafe {
                    low.GetEnvelopePoint(
                        envelope,
                        point_idx,
                        &mut point_time,
                        &mut point_value,
                        &mut _shape,
                        &mut _tension,
                        &mut _selected,
                    )
                };

                if !success {
                    continue;
                }

                // Convert point time to screen X
                let point_screen_x = ((point_time - start_time) * arrange_zoom) as i32;

                // Check if mouse X is within hit range of point X
                if arrange_x >= point_screen_x - ENV_HIT_POINT
                    && arrange_x <= point_screen_x + ENV_HIT_POINT_LEFT
                {
                    // We're near this point's X position
                    // TODO: Also check Y position against envelope value
                    // For now, report point hit if X is close
                    return (
                        MouseModifierContext::EnvelopePoint,
                        format!(
                            "window: arrange, segment: envelope, details: env_point (idx: {}, time: {:.3})",
                            point_idx, point_time
                        ),
                    );
                }
            }
        }

        // Not over a point - check if over envelope segment
        // Evaluate envelope at mouse position to get Y value
        let mut env_value: f64 = 0.0;
        let mut _dydx: f64 = 0.0;
        let mut _ddydx: f64 = 0.0;
        let mut _dddydx: f64 = 0.0;

        let eval_success = unsafe {
            low.Envelope_Evaluate(
                envelope,
                mouse_time,
                0.0, // sample rate (0 = use project)
                0,   // samples ahead
                &mut env_value,
                &mut _dydx,
                &mut _ddydx,
                &mut _dddydx,
            )
        };

        if eval_success > 0 {
            // Envelope exists at this time - we're over a segment
            return (
                MouseModifierContext::EnvelopeSegment,
                format!(
                    "window: arrange, segment: envelope, details: env_segment (time: {:.3}, value: {:.3})",
                    mouse_time, env_value
                ),
            );
        }
    }

    // Default: in envelope lane but not over specific point/segment
    (
        MouseModifierContext::Envelope,
        "window: arrange, segment: envelope, details: empty".to_string(),
    )
}

/// Detect if mouse is over a razor edit area
fn detect_razor_edit(
    track: *mut reaper_low::raw::MediaTrack,
    mouse_time: f64,
    zoom: f64,
    mouse_x: i32,
    start_time: f64,
    low: &reaper_low::Reaper,
) -> Option<(MouseModifierContext, String)> {
    // Get razor edits string from track
    // Format: "start end isEnvelope GUID, start2 end2 isEnvelope GUID, ..."
    let mut buf = [0u8; 4096];
    let success = unsafe {
        low.GetSetMediaTrackInfo_String(
            track,
            c"P_RAZOREDITS".as_ptr(),
            buf.as_mut_ptr() as *mut i8,
            false,
        )
    };

    if !success {
        return None;
    }

    // Convert to string and parse
    let razor_str = std::ffi::CStr::from_bytes_until_nul(&buf)
        .ok()?
        .to_str()
        .ok()?;

    if razor_str.is_empty() {
        return None;
    }

    const RAZOR_EDGE_THRESHOLD: i32 = 5;

    // Parse each razor edit area
    for area in razor_str.split(' ') {
        let parts: Vec<&str> = area.split(' ').collect();
        if parts.len() < 2 {
            continue;
        }

        let area_start: f64 = parts[0].parse().ok()?;
        let area_end: f64 = parts[1].parse().ok()?;
        let is_envelope = parts.get(2).map(|s| *s == "1").unwrap_or(false);

        // Convert to screen coordinates
        let area_start_x = ((area_start - start_time) * zoom) as i32;
        let area_end_x = ((area_end - start_time) * zoom) as i32;

        // Check left edge
        if mouse_x >= area_start_x - RAZOR_EDGE_THRESHOLD
            && mouse_x <= area_start_x + RAZOR_EDGE_THRESHOLD
        {
            return Some((
                MouseModifierContext::RazorEditEdge,
                format!(
                    "window: arrange, segment: razor_edit, details: edge_left (start: {:.3})",
                    area_start
                ),
            ));
        }

        // Check right edge
        if mouse_x >= area_end_x - RAZOR_EDGE_THRESHOLD
            && mouse_x <= area_end_x + RAZOR_EDGE_THRESHOLD
        {
            return Some((
                MouseModifierContext::RazorEditEdge,
                format!(
                    "window: arrange, segment: razor_edit, details: edge_right (end: {:.3})",
                    area_end
                ),
            ));
        }

        // Check if inside the area
        if mouse_time >= area_start && mouse_time <= area_end {
            let ctx = if is_envelope {
                MouseModifierContext::RazorEditEnvelope
            } else {
                MouseModifierContext::RazorEdit
            };
            return Some((
                ctx,
                format!(
                    "window: arrange, segment: razor_edit, details: area (start: {:.3}, end: {:.3}, env: {})",
                    area_start, area_end, is_envelope
                ),
            ));
        }
    }

    None
}

// endregion: --- Arrange Detection

// region: --- Item Detection

/// Detect which part of an item the mouse is over (with detailed info)
fn detect_item_context_detailed(
    item: *mut MediaItem,
    arrange_x: i32,
    arrange_y: i32,
    medium: &reaper_medium::Reaper,
    result: &mut MouseContextResult,
) -> MouseModifierContext {
    use crate::input::reaper_windows;
    use reaper_low::Swell;

    let low = medium.low();
    let swell = Swell::get();

    // Get item screen bounds using I_LASTY and I_LASTH
    // These give us the last drawn position relative to arrange view
    let item_last_y = unsafe { low.GetMediaItemInfo_Value(item, c"I_LASTY".as_ptr()) } as i32;
    let item_last_h = unsafe { low.GetMediaItemInfo_Value(item, c"I_LASTH".as_ptr()) } as i32;

    // Get item time info for horizontal bounds
    let item_pos = unsafe { low.GetMediaItemInfo_Value(item, c"D_POSITION".as_ptr()) };
    let item_len = unsafe { low.GetMediaItemInfo_Value(item, c"D_LENGTH".as_ptr()) };

    // Get fade lengths (in seconds)
    let fade_in_len = unsafe { low.GetMediaItemInfo_Value(item, c"D_FADEINLEN".as_ptr()) };
    let fade_out_len = unsafe { low.GetMediaItemInfo_Value(item, c"D_FADEOUTLEN".as_ptr()) };

    // Convert time to screen X using arrange view
    let arrange_hwnd = match reaper_windows::get_arrange_wnd(medium) {
        Some(h) => h,
        None => return MouseModifierContext::Item,
    };

    // Get visible time range and calculate pixels per second
    let mut start_time: f64 = 0.0;
    let mut end_time: f64 = 0.0;
    unsafe {
        low.GetSet_ArrangeView2(
            std::ptr::null_mut(),
            false,
            0,
            0,
            &mut start_time,
            &mut end_time,
        );
    }

    let mut arrange_rect = reaper_low::raw::RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    unsafe {
        swell.GetClientRect(arrange_hwnd, &mut arrange_rect);
    }

    let arrange_width = arrange_rect.right - arrange_rect.left;
    let time_range = end_time - start_time;
    let px_per_sec = if time_range > 0.0 {
        arrange_width as f64 / time_range
    } else {
        1.0
    };

    // Calculate item screen bounds
    let item_screen_left = ((item_pos - start_time) * px_per_sec) as i32;
    let item_screen_right = ((item_pos + item_len - start_time) * px_per_sec) as i32;
    let item_screen_top = item_last_y;
    let item_screen_bottom = item_last_y + item_last_h;

    // Calculate fade lengths in pixels
    let fade_in_px = (fade_in_len * px_per_sec) as i32;
    let fade_out_px = (fade_out_len * px_per_sec) as i32;

    // Calculate relative mouse position within item
    let rel_x = arrange_x - item_screen_left;
    let rel_y = arrange_y - item_screen_top;
    let item_width = item_screen_right - item_screen_left;
    let item_height = item_last_h;

    // Store item info
    result.item_info = Some(ItemHitInfo {
        item,
        screen_left: item_screen_left,
        screen_right: item_screen_right,
        screen_top: item_screen_top,
        screen_bottom: item_screen_bottom,
        height: item_height,
        rel_x,
        rel_y,
        fade_in_px,
        fade_out_px,
    });

    result.details = format!(
        "Item bounds: ({}, {}) - ({}, {}), Mouse rel: ({}, {}), Fades: in={}px out={}px",
        item_screen_left,
        item_screen_top,
        item_screen_right,
        item_screen_bottom,
        rel_x,
        rel_y,
        fade_in_px,
        fade_out_px
    );

    // Now determine which context based on relative position

    // Check for edge (left or right)
    if rel_x < EDGE_THRESHOLD_PX {
        return MouseModifierContext::ItemLeftEdge;
    }
    if rel_x > item_width - EDGE_THRESHOLD_PX {
        return MouseModifierContext::ItemRightEdge;
    }

    // Check for fade handles (upper corners, within fade area)
    if rel_y < FADE_HANDLE_HEIGHT_PX {
        if rel_x < fade_in_px.max(EDGE_THRESHOLD_PX) {
            return MouseModifierContext::ItemFadeIn;
        }
        if rel_x > item_width - fade_out_px.max(EDGE_THRESHOLD_PX) {
            return MouseModifierContext::ItemFadeOut;
        }
    }

    // Check for lower half
    let lower_half_start = (item_height as f64 * LOWER_HALF_THRESHOLD) as i32;
    if rel_y >= lower_half_start {
        return MouseModifierContext::ItemLower;
    }

    // Check for stretch markers
    // Get the active take for this item
    let take = unsafe { low.GetActiveTake(item) };
    if !take.is_null() {
        let num_stretch_markers = unsafe { low.GetTakeNumStretchMarkers(take) };

        if num_stretch_markers > 0 {
            // Check each stretch marker
            for idx in 0..num_stretch_markers {
                let mut sm_pos: f64 = 0.0;
                let mut _src_pos: f64 = 0.0;

                let sm_idx =
                    unsafe { low.GetTakeStretchMarker(take, idx, &mut sm_pos, &mut _src_pos) };

                if sm_idx >= 0 {
                    // Convert stretch marker position to screen X
                    let sm_screen_x = ((sm_pos) * px_per_sec) as i32;

                    // Check if mouse is near this stretch marker
                    const STRETCH_MARKER_THRESHOLD: i32 = 5;
                    if rel_x >= sm_screen_x - STRETCH_MARKER_THRESHOLD
                        && rel_x <= sm_screen_x + STRETCH_MARKER_THRESHOLD
                    {
                        return MouseModifierContext::ItemStretchMarker;
                    }
                }
            }
        }
    }

    // Default: item body
    MouseModifierContext::Item
}

// endregion: --- Item Detection

// region: --- TCP Detection

/// Check if mouse is over TCP track
fn check_tcp_context(
    hwnd_under_mouse: reaper_low::raw::HWND,
    tcp_hwnd: reaper_low::raw::HWND,
    is_container: bool,
    pt: &reaper_low::raw::POINT,
    medium: &reaper_medium::Reaper,
    swell: &reaper_low::Swell,
) -> Option<(
    *mut reaper_low::raw::MediaTrack,
    crate::input::reaper_windows::HwndToTrackContext,
)> {
    use crate::input::reaper_windows;

    let hwnd_parent = unsafe { swell.GetParent(hwnd_under_mouse) };

    if is_container {
        if hwnd_under_mouse == tcp_hwnd {
            // Use hwnd_to_track to find which track
            return reaper_windows::hwnd_to_track(hwnd_under_mouse, *pt, medium);
        }
    } else {
        if hwnd_parent == tcp_hwnd {
            return reaper_windows::hwnd_to_track(hwnd_under_mouse, *pt, medium);
        }
    }

    None
}

// endregion: --- TCP Detection

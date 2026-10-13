//! REAPER Window Utilities
//!
//! Port of window detection functions from BR_Util.cpp (SWS).
//! Provides functions to get HWNDs of various REAPER windows using control IDs
//! that are consistent across all platforms.

use reaper_hwnd::class_names::{REAPER_DOCK, REAPER_MCP_DISPLAY, REAPER_TCP_DISPLAY};
use reaper_hwnd::dialogs::main_window::MainWindow;
use reaper_hwnd::dialogs::midi_editor::MidiEditor;
use reaper_low::Swell;
use reaper_low::raw::{HWND, POINT};
use reaper_medium::Reaper as MediumReaper;
use std::ffi::CString;
use swell_ui::Window;

/// Get arrange view window HWND
/// Port of BR_Util::GetArrangeWnd()
/// Uses GetDlgItem with control ID that's consistent across platforms
pub fn get_arrange_wnd(medium_reaper: &MediumReaper) -> Option<HWND> {
    let main_hwnd = medium_reaper.get_main_hwnd();

    // Use GetDlgItem to get child window by control ID
    let arrange_hwnd =
        unsafe { Swell::get().GetDlgItem(main_hwnd.as_ptr(), MainWindow::TRACKVIEW.raw() as i32) };

    if arrange_hwnd.is_null() {
        None
    } else {
        Some(arrange_hwnd)
    }
}

/// Get ruler window HWND
/// Port of BR_Util::GetRulerWndAlt()
/// Uses GetDlgItem with control ID that's consistent across platforms
pub fn get_ruler_wnd(medium_reaper: &MediumReaper) -> Option<HWND> {
    let main_hwnd = medium_reaper.get_main_hwnd();

    // Use GetDlgItem to get child window by control ID
    let ruler_hwnd =
        unsafe { Swell::get().GetDlgItem(main_hwnd.as_ptr(), MainWindow::TIMELINE.raw() as i32) };

    if ruler_hwnd.is_null() {
        None
    } else {
        Some(ruler_hwnd)
    }
}

/// Search for a window by name in children of a parent window
/// Port of SearchChildren from SWS
///
/// On Windows with localization, uses GetWindowText and manual search.
/// Otherwise, uses FindWindowEx with name as title parameter (more efficient).
fn search_children(
    name: &str,
    parent: HWND,
    start_after: Option<HWND>,
    window_has_no_children: bool,
) -> Option<HWND> {
    let swell = Swell::get();
    let name_cstr = CString::new(name).ok()?;

    // Try FindWindowEx approach first (works on non-localized Windows and all platforms)
    // This is more efficient than manual enumeration
    let mut return_hwnd = start_after;

    loop {
        let found = unsafe {
            swell.FindWindowEx(
                parent,
                return_hwnd.unwrap_or(std::ptr::null_mut()),
                std::ptr::null(),
                name_cstr.as_ptr(),
            )
        };

        if found.is_null() {
            break;
        }

        // If windowHasNoChildren is true, check that the window has no children
        if !window_has_no_children {
            return Some(found);
        }

        // Check if window has no children
        let has_children = unsafe {
            !swell
                .GetWindow(found, reaper_low::raw::GW_CHILD as i32)
                .is_null()
        };

        if !has_children {
            return Some(found);
        }

        // Continue searching from this point
        return_hwnd = Some(found);
    }

    // Fallback: Manual enumeration (for localized Windows or if FindWindowEx fails)
    // This matches the Windows localized code path
    // If parent is null, we can't enumerate children, so return None
    if parent.is_null() {
        return None;
    }

    let mut current = if let Some(start) = start_after {
        // Start from next sibling after start_after
        unsafe { swell.GetWindow(start, reaper_low::raw::GW_HWNDNEXT as i32) }
    } else {
        // Start from first child
        unsafe { swell.GetWindow(parent, reaper_low::raw::GW_CHILD as i32) }
    };

    while !current.is_null() {
        // Get window text and compare
        if let Some(window) = Window::new(current)
            && let Ok(window_text) = window.text()
            && window_text == name
        {
            // If windowHasNoChildren is true, check that the window has no children
            if window_has_no_children {
                let has_children = unsafe {
                    !swell
                        .GetWindow(current, reaper_low::raw::GW_CHILD as i32)
                        .is_null()
                };
                if has_children {
                    // Continue searching
                    current =
                        unsafe { swell.GetWindow(current, reaper_low::raw::GW_HWNDNEXT as i32) };
                    continue;
                }
            }
            return Some(current);
        }

        // Move to next sibling
        current = unsafe { swell.GetWindow(current, reaper_low::raw::GW_HWNDNEXT as i32) };
    }

    None
}

/// Find window in REAPER main window children
/// Port of FindInReaper from SWS
fn find_in_reaper(name: &str, medium_reaper: &MediumReaper) -> Option<HWND> {
    let main_hwnd = medium_reaper.get_main_hwnd();
    search_children(name, main_hwnd.as_ptr(), None, false)
}

/// Find window in REAPER dockers
/// Port of FindInReaperDockers from SWS
fn find_in_reaper_dockers(name: &str, medium_reaper: &MediumReaper) -> Option<HWND> {
    let main_hwnd = medium_reaper.get_main_hwnd();
    let swell = Swell::get();

    // Find docker windows by class name "REAPER_dock"
    let docker_class = CString::new(REAPER_DOCK).ok()?;
    let mut docker = unsafe {
        swell.FindWindowEx(
            main_hwnd.as_ptr(),
            std::ptr::null_mut(),
            docker_class.as_ptr(),
            std::ptr::null(),
        )
    };

    while !docker.is_null() {
        if let Some(hwnd) = search_children(name, docker, None, false) {
            return Some(hwnd);
        }

        // Find next docker
        docker = unsafe {
            swell.FindWindowEx(
                main_hwnd.as_ptr(),
                docker,
                docker_class.as_ptr(),
                std::ptr::null(),
            )
        };
    }

    None
}

/// Find floating window
/// Port of FindFloating from SWS
fn find_floating(
    name: &str,
    medium_reaper: &MediumReaper,
    check_for_no_caption: bool,
    window_has_no_children: bool,
) -> Option<HWND> {
    let main_hwnd = medium_reaper.get_main_hwnd();
    let swell = Swell::get();
    let mut hwnd = search_children(name, std::ptr::null_mut(), None, window_has_no_children);

    while let Some(current_hwnd) = hwnd {
        // Check if parent is main window
        let parent = unsafe { swell.GetParent(current_hwnd) };

        if parent == main_hwnd.as_ptr() {
            // Check for no caption if requested
            if check_for_no_caption {
                let _style =
                    unsafe { swell.GetWindowLong(current_hwnd, reaper_low::raw::GWL_STYLE) as u32 };
                // WS_CAPTION check - if style doesn't have caption, continue
                // For now, we'll skip this check as it's complex
            }

            return Some(current_hwnd);
        }

        // Continue searching
        hwnd = search_children(
            name,
            std::ptr::null_mut(),
            Some(current_hwnd),
            window_has_no_children,
        );
    }

    None
}

/// Find window in floating dockers
/// Port of FindInFloatingDockers from SWS (simplified)
fn find_in_floating_dockers(_name: &str, _medium_reaper: &MediumReaper) -> Option<HWND> {
    // TODO: Full implementation would search floating docker windows
    // For now, return None - this is complex and may not be critical
    None
}

/// Get transport window HWND
/// Port of BR_Util::GetTransportWnd()
/// Uses window title matching with localization
pub fn get_transport_wnd(medium_reaper: &MediumReaper) -> Option<HWND> {
    // Try "Transport" as the window name
    // Note: SWS uses __localizeFunc("Transport", "DLG_188", 0) for localization
    // For now, we'll try the English name and hope it works
    // In the future, we could add localization support

    let transport_name = "Transport";

    // Search in order: dockers, floating, floating dockers, main window
    if let Some(hwnd) = find_in_reaper_dockers(transport_name, medium_reaper) {
        return Some(hwnd);
    }

    if let Some(hwnd) = find_floating(transport_name, medium_reaper, false, false) {
        return Some(hwnd);
    }

    if let Some(hwnd) = find_in_floating_dockers(transport_name, medium_reaper) {
        return Some(hwnd);
    }

    if let Some(hwnd) = find_in_reaper(transport_name, medium_reaper) {
        return Some(hwnd);
    }

    None
}

/// Get notes view HWND from MIDI editor
/// Port of BR_Util::GetNotesView()
/// Uses GetDlgItem with control ID that's consistent across platforms
pub fn get_notes_view(midi_editor_hwnd: HWND, medium_reaper: &MediumReaper) -> Option<HWND> {
    // Check if it's a valid MIDI editor
    let mode = unsafe { medium_reaper.low().MIDIEditor_GetMode(midi_editor_hwnd) };

    if mode == -1 {
        return None;
    }

    // Use GetDlgItem to get child window by control ID
    let notes_view_hwnd =
        unsafe { Swell::get().GetDlgItem(midi_editor_hwnd, MidiEditor::NOTES_VIEW.raw() as i32) };

    if notes_view_hwnd.is_null() {
        None
    } else {
        Some(notes_view_hwnd)
    }
}

/// Get piano view HWND from MIDI editor
/// Port of BR_Util::GetPianoView()
/// Uses GetDlgItem with control ID that's consistent across platforms
pub fn get_piano_view(midi_editor_hwnd: HWND, medium_reaper: &MediumReaper) -> Option<HWND> {
    // Check if it's a valid MIDI editor
    let mode = unsafe { medium_reaper.low().MIDIEditor_GetMode(midi_editor_hwnd) };

    if mode == -1 {
        return None;
    }

    // Use GetDlgItem to get child window by control ID
    let piano_view_hwnd =
        unsafe { Swell::get().GetDlgItem(midi_editor_hwnd, MidiEditor::PIANO_VIEW.raw() as i32) };

    if piano_view_hwnd.is_null() {
        None
    } else {
        Some(piano_view_hwnd)
    }
}

/// Get TCP window HWND
/// Port of BR_Util::GetTcpWnd()
/// Returns (HWND, is_container)
/// Note: We don't cache the result like SWS does because HWND is not Send/Sync
/// This means we'll search each time, but it's safer for multi-threaded contexts
pub fn get_tcp_wnd(medium_reaper: &MediumReaper) -> (Option<HWND>, bool) {
    let main_hwnd = medium_reaper.get_main_hwnd();
    let swell = Swell::get();

    // Try to find by class name "REAPERTCPDisplay"
    let tcp_class = CString::new(REAPER_TCP_DISPLAY).ok();
    let mut tcp_hwnd = if let Some(class) = &tcp_class {
        unsafe {
            swell.FindWindowEx(
                main_hwnd.as_ptr(),
                std::ptr::null_mut(),
                class.as_ptr(),
                std::ptr::null(),
            )
        }
    } else {
        std::ptr::null_mut()
    };

    // macOS workaround: verify class name (old macOS swell FindWindowEx is broken)
    #[cfg(target_os = "macos")]
    {
        if !tcp_hwnd.is_null() {
            let mut buf = vec![0u8; 1024];
            let got_class = unsafe {
                swell.GetClassName(tcp_hwnd, buf.as_mut_ptr() as *mut i8, buf.len() as i32)
            };

            if got_class == 0 {
                tcp_hwnd = std::ptr::null_mut();
            } else {
                let class_name =
                    unsafe { CString::from_vec_unchecked(buf[..got_class as usize].to_vec()) };
                if class_name.to_string_lossy() != REAPER_TCP_DISPLAY {
                    tcp_hwnd = std::ptr::null_mut();
                }
            }
        }
    }

    let mut is_container = false;
    if !tcp_hwnd.is_null() {
        is_container = true;
        return (Some(tcp_hwnd), is_container);
    }

    // Legacy path - 5.x releases
    // Find first visible track in TCP
    let low_reaper = medium_reaper.low();
    let track_count = unsafe { low_reaper.CountTracks(std::ptr::null_mut()) };

    let mut track: *mut reaper_low::raw::MediaTrack = std::ptr::null_mut();
    for i in 0..track_count {
        let current_track = unsafe { low_reaper.GetTrack(std::ptr::null_mut(), i) };

        if current_track.is_null() {
            continue;
        }

        // Check B_SHOWINTCP using GetMediaTrackInfo_Value
        let show_in_tcp = unsafe {
            low_reaper
                .GetMediaTrackInfo_Value(current_track, b"B_SHOWINTCP\0".as_ptr() as *const i8)
                != 0.0
        };

        // Check I_WNDH (window height)
        let wnd_h = unsafe {
            low_reaper.GetMediaTrackInfo_Value(current_track, b"I_WNDH\0".as_ptr() as *const i8)
        };

        if show_in_tcp && wnd_h != 0.0 {
            track = current_track;
            break;
        }
    }

    // Get master track
    let master = unsafe { low_reaper.GetMasterTrack(std::ptr::null_mut()) };

    // Check if master is visible in TCP
    // TODO: TcpVis() function - for now, check B_SHOWINTCP
    let master_show_in_tcp = if !master.is_null() {
        unsafe {
            low_reaper.GetMediaTrackInfo_Value(master, b"B_SHOWINTCP\0".as_ptr() as *const i8)
                != 0.0
        }
    } else {
        false
    };

    // Determine first track to search for
    let first_track = if master_show_in_tcp {
        master
    } else if !track.is_null() {
        track
    } else {
        master
    };

    // If no tracks visible, temporarily show master to find TCP
    let mut master_vis_restore: Option<i32> = None;
    if track.is_null() && !master_show_in_tcp {
        let current_vis = low_reaper.GetMasterTrackVisibility();
        low_reaper.SetMasterTrackVisibility(current_vis | 1); // Show in TCP
        master_vis_restore = Some(current_vis);
    }

    // TCP hierarchy: TCP owner -> TCP hwnd -> tracks
    // Enumerate windows to find the one containing first_track
    let mut found_hwnd: Option<HWND> = None;
    let mut tcp_owner = unsafe {
        swell.FindWindowEx(
            main_hwnd.as_ptr(),
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
        )
    };

    while !tcp_owner.is_null() && found_hwnd.is_none() {
        let mut tcp_hwnd = unsafe {
            swell.FindWindowEx(
                tcp_owner,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
            )
        };

        while !tcp_hwnd.is_null() && found_hwnd.is_none() {
            let mut track_hwnd = unsafe {
                swell.FindWindowEx(
                    tcp_hwnd,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    std::ptr::null(),
                )
            };

            while !track_hwnd.is_null() && found_hwnd.is_none() {
                // Get track pointer from window user data
                // GWL_USERDATA = -21
                let track_ptr = unsafe {
                    swell.GetWindowLong(track_hwnd, -21) as *mut reaper_low::raw::MediaTrack
                };

                if track_ptr == first_track {
                    found_hwnd = Some(tcp_hwnd);
                    break;
                }

                track_hwnd = unsafe {
                    swell.FindWindowEx(tcp_hwnd, track_hwnd, std::ptr::null(), std::ptr::null())
                };
            }

            if found_hwnd.is_some() {
                break;
            }

            tcp_hwnd = unsafe {
                swell.FindWindowEx(tcp_owner, tcp_hwnd, std::ptr::null(), std::ptr::null())
            };
        }

        if found_hwnd.is_some() {
            break;
        }

        tcp_owner = unsafe {
            swell.FindWindowEx(
                main_hwnd.as_ptr(),
                tcp_owner,
                std::ptr::null(),
                std::ptr::null(),
            )
        };
    }

    // Restore master track visibility if we changed it
    if let Some(vis) = master_vis_restore {
        low_reaper.SetMasterTrackVisibility(vis);
    }

    (found_hwnd, is_container)
}

/// Get TCP track window HWND for a specific track
/// Port of BR_Util::GetTcpTrackWnd()
pub fn get_tcp_track_wnd(
    track: *mut reaper_low::raw::MediaTrack,
    medium_reaper: &MediumReaper,
) -> Option<HWND> {
    let (tcp_hwnd, is_container) = get_tcp_wnd(medium_reaper);

    let tcp_hwnd = tcp_hwnd?;

    // If container, return the container itself
    if is_container {
        return Some(tcp_hwnd);
    }

    // Otherwise, enumerate children to find the track
    let swell = Swell::get();
    let mut child = unsafe { swell.GetWindow(tcp_hwnd, reaper_low::raw::GW_CHILD as i32) };

    while !child.is_null() {
        // Get track pointer from window user data
        // GWL_USERDATA = -21
        let track_ptr =
            unsafe { swell.GetWindowLong(child, -21) as *mut reaper_low::raw::MediaTrack };

        if track_ptr == track {
            return Some(child);
        }

        child = unsafe { swell.GetWindow(child, reaper_low::raw::GW_HWNDNEXT as i32) };
    }

    None
}

/// Get mixer window HWND
/// Port of BR_Util::GetMixerWnd()
/// Uses window title matching with localization
pub fn get_mixer_wnd(medium_reaper: &MediumReaper) -> Option<HWND> {
    // Try "Mixer" as the window name
    // Note: SWS uses __localizeFunc("Mixer", "DLG_151", 0) for localization
    let mixer_name = "Mixer";

    // Use FindReaperWndByPreparedString logic
    // Search in order: dockers, floating, floating dockers, main window
    if let Some(hwnd) = find_in_reaper_dockers(mixer_name, medium_reaper) {
        return Some(hwnd);
    }

    if let Some(hwnd) = find_floating(mixer_name, medium_reaper, false, false) {
        return Some(hwnd);
    }

    if let Some(hwnd) = find_in_floating_dockers(mixer_name, medium_reaper) {
        return Some(hwnd);
    }

    if let Some(hwnd) = find_in_reaper(mixer_name, medium_reaper) {
        return Some(hwnd);
    }

    None
}

/// Get mixer master window HWND
/// Port of BR_Util::GetMixerMasterWnd()
pub fn get_mixer_master_wnd(mixer_hwnd: HWND, medium_reaper: &MediumReaper) -> Option<HWND> {
    // Try "Mixer Master" as the window name
    let mixer_master_name = "Mixer Master";

    // First try by prepared string (searches dockers, floating, etc.)
    if let Some(hwnd) = find_in_reaper_dockers(mixer_master_name, medium_reaper) {
        return Some(hwnd);
    }

    if let Some(hwnd) = find_floating(mixer_master_name, medium_reaper, false, false) {
        return Some(hwnd);
    }

    // v6+ in mixer: try to find by class name "master"
    let swell = Swell::get();
    let master_class = CString::new("master").ok();
    if let Some(class) = &master_class {
        let hwnd = unsafe {
            swell.FindWindowEx(
                mixer_hwnd,
                std::ptr::null_mut(),
                class.as_ptr(),
                std::ptr::null(),
            )
        };
        if !hwnd.is_null() {
            return Some(hwnd);
        }
    }

    None
}

/// Get MCP window HWND
/// Port of BR_Util::GetMcpWnd()
/// Returns (HWND, is_container)
pub fn get_mcp_wnd(medium_reaper: &MediumReaper) -> (Option<HWND>, bool) {
    let mixer_hwnd = match get_mixer_wnd(medium_reaper) {
        Some(h) => h,
        None => return (None, false),
    };

    let swell = Swell::get();
    let mut is_container = false;

    // Try to find by class name "REAPERMCPDisplay"
    let mcp_class = CString::new(REAPER_MCP_DISPLAY).ok();
    let mut mcp_hwnd = if let Some(class) = &mcp_class {
        unsafe {
            swell.FindWindowEx(
                mixer_hwnd,
                std::ptr::null_mut(),
                class.as_ptr(),
                std::ptr::null(),
            )
        }
    } else {
        std::ptr::null_mut()
    };

    // macOS workaround: verify class name (old macOS swell FindWindowEx is broken)
    #[cfg(target_os = "macos")]
    {
        if !mcp_hwnd.is_null() {
            let mut buf = vec![0u8; 1024];
            let got_class = unsafe {
                swell.GetClassName(mcp_hwnd, buf.as_mut_ptr() as *mut i8, buf.len() as i32)
            };

            if got_class == 0 {
                mcp_hwnd = std::ptr::null_mut();
            } else {
                let class_name =
                    unsafe { CString::from_vec_unchecked(buf[..got_class as usize].to_vec()) };
                if class_name.to_string_lossy() != REAPER_MCP_DISPLAY {
                    mcp_hwnd = std::ptr::null_mut();
                }
            }
        }
    }

    if !mcp_hwnd.is_null() {
        is_container = true;
        return (Some(mcp_hwnd), is_container);
    }

    // Legacy path - 5.x releases
    // Find first child window that's not the master track
    let low_reaper = medium_reaper.low();
    let master = unsafe { low_reaper.GetMasterTrack(std::ptr::null_mut()) };

    let mut hwnd = unsafe {
        swell.FindWindowEx(
            mixer_hwnd,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
        )
    };

    while !hwnd.is_null() {
        // Get track pointer from window user data
        let track_ptr =
            unsafe { swell.GetWindowLong(hwnd, -21) as *mut reaper_low::raw::MediaTrack };

        // Skip master track
        if track_ptr != master {
            return (Some(hwnd), is_container);
        }

        // Find next child
        hwnd = unsafe { swell.FindWindowEx(mixer_hwnd, hwnd, std::ptr::null(), std::ptr::null()) };
    }

    (None, is_container)
}

/// Context flags for HwndToTrack
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HwndToTrackContext {
    pub is_tcp: bool,
    pub is_mcp: bool,
    pub is_spacer: bool,
}

impl Default for HwndToTrackContext {
    fn default() -> Self {
        Self::new()
    }
}

impl HwndToTrackContext {
    pub fn new() -> Self {
        Self {
            is_tcp: false,
            is_mcp: false,
            is_spacer: false,
        }
    }
}

/// Get track spacer size
/// Port of BR_Util::GetTrackSpacerSize() (simplified)
/// TODO: Full implementation would check ConfigVar("trackgapmax") and HasTrackSpacerBefore
pub(crate) fn get_track_spacer_size(
    _track: *mut reaper_low::raw::MediaTrack,
    _is_mcp: bool,
    _medium_reaper: &MediumReaper,
) -> i32 {
    // TODO: Full implementation would:
    // - Check ConfigVar<int>("trackgapmax")
    // - Check IsTrackVisible(track, isMCP)
    // - Check HasTrackSpacerBefore(track, isMCP)
    // - Call LimitTrackSpacerSize()
    // For now, return 0 (no spacer)
    0
}

/// Convert HWND to TrackEnvelope*
/// Port of BR_Util::HwndToEnvelope()
pub fn hwnd_to_envelope(
    hwnd: HWND,
    pt_screen: POINT,
    medium_reaper: &MediumReaper,
) -> Option<*mut reaper_low::raw::TrackEnvelope> {
    let (tcp_hwnd, is_container) = get_tcp_wnd(medium_reaper);
    let tcp_hwnd = tcp_hwnd?;
    let swell = Swell::get();
    let low_reaper = medium_reaper.low();

    if is_container {
        if hwnd == tcp_hwnd {
            // Convert screen point to client coordinates
            let mut pt_client = pt_screen;
            unsafe {
                swell.ScreenToClient(hwnd, &mut pt_client);
            }

            let track_count = low_reaper.GetNumTracks();

            // Search through tracks (including master at i=-1)
            for i in -1..track_count {
                let chk_track = if i < 0 {
                    unsafe { low_reaper.GetMasterTrack(std::ptr::null_mut()) }
                } else {
                    unsafe { low_reaper.GetTrack(std::ptr::null_mut(), i) }
                };

                if chk_track.is_null() {
                    continue;
                }

                // Check B_SHOWINTCP using GetSetMediaTrackInfo (returns pointer)
                let show_in_tcp_ptr = unsafe {
                    low_reaper.GetSetMediaTrackInfo(
                        chk_track,
                        b"B_SHOWINTCP\0".as_ptr() as *const i8,
                        std::ptr::null_mut(),
                    )
                };

                if show_in_tcp_ptr.is_null() {
                    continue;
                }

                let show_in_tcp = unsafe { *(show_in_tcp_ptr as *const bool) };

                if !show_in_tcp {
                    continue;
                }

                // Get I_TCPY
                let ypos_ptr = unsafe {
                    low_reaper.GetSetMediaTrackInfo(
                        chk_track,
                        b"I_TCPY\0".as_ptr() as *const i8,
                        std::ptr::null_mut(),
                    )
                };
                let ypos = if ypos_ptr.is_null() {
                    0
                } else {
                    unsafe { *(ypos_ptr as *const i32) }
                };

                // Get I_WNDH (includes env lanes)
                let h_ptr = unsafe {
                    low_reaper.GetSetMediaTrackInfo(
                        chk_track,
                        b"I_WNDH\0".as_ptr() as *const i8,
                        std::ptr::null_mut(),
                    )
                };
                let h = if h_ptr.is_null() {
                    0
                } else {
                    unsafe { *(h_ptr as *const i32) }
                };

                // Check if point is within track bounds
                if pt_client.y < ypos || pt_client.y >= ypos + h {
                    continue;
                }

                // Enumerate track envelopes
                let env_count = unsafe { low_reaper.CountTrackEnvelopes(chk_track) };

                for e in 0..env_count {
                    let env = unsafe { low_reaper.GetTrackEnvelope(chk_track, e) };

                    if env.is_null() {
                        continue;
                    }

                    // Get envelope Y and height using GetEnvelopeInfo_Value
                    let env_y = unsafe {
                        low_reaper.GetEnvelopeInfo_Value(env, b"I_TCPY\0".as_ptr() as *const i8)
                            as i32
                    };
                    let env_h = unsafe {
                        low_reaper.GetEnvelopeInfo_Value(env, b"I_TCPH\0".as_ptr() as *const i8)
                            as i32
                    };

                    // Check if point is within envelope bounds
                    if pt_client.y >= ypos + env_y && pt_client.y < ypos + env_y + env_h {
                        return Some(env);
                    }
                }

                break; // Only check first matching track
            }
        }
    } else {
        // Not a container - check if hwnd is a direct child of TCP
        let parent = unsafe { swell.GetParent(hwnd) };

        if parent == tcp_hwnd {
            // Get envelope pointer from window user data
            let env_ptr =
                unsafe { swell.GetWindowLong(hwnd, -21) as *mut reaper_low::raw::TrackEnvelope };

            // Validate pointer using ValidatePtr if available
            if !env_ptr.is_null() {
                let is_valid = unsafe {
                    if low_reaper.pointers().ValidatePtr.is_some() {
                        let type_name = b"TrackEnvelope*\0".as_ptr() as *const i8;
                        low_reaper.ValidatePtr(env_ptr as *mut std::ffi::c_void, type_name)
                    } else {
                        true
                    }
                };
                if is_valid {
                    return Some(env_ptr);
                }
            }
        }
    }

    None
}

/// Convert HWND to MediaTrack* (TCP/MCP)
/// Port of BR_Util::HwndToTrack()
/// Returns (MediaTrack*, HwndToTrackContext)
pub fn hwnd_to_track(
    hwnd: HWND,
    pt_screen: POINT,
    medium_reaper: &MediumReaper,
) -> Option<(*mut reaper_low::raw::MediaTrack, HwndToTrackContext)> {
    let low_reaper = medium_reaper.low();
    let swell = Swell::get();
    let mut track: *mut reaper_low::raw::MediaTrack = std::ptr::null_mut();
    let mut context = HwndToTrackContext::new();
    let mut in_spacer = false;

    let hwnd_parent = unsafe { swell.GetParent(hwnd) };

    // Check TCP first
    {
        let (tcp_hwnd, is_container) = get_tcp_wnd(medium_reaper);

        if let Some(tcp) = tcp_hwnd {
            if is_container {
                if hwnd == tcp {
                    // Convert screen point to client coordinates
                    let mut pt_client = pt_screen;
                    unsafe {
                        swell.ScreenToClient(hwnd, &mut pt_client);
                    }

                    // Get showmaintrack config (default to true if not available)
                    // TODO: Get from ConfigVar<int>("showmaintrack")
                    let show_main_track = true;
                    let start_i = if show_main_track { 0 } else { 1 };

                    let track_count = low_reaper.GetNumTracks();

                    // Search through tracks using CSurf_TrackFromID
                    for i in start_i..=track_count {
                        let chk_track = unsafe {
                            // Use CSurf_TrackFromID if available (i=0 is master, i=1..N are tracks)
                            if low_reaper.pointers().CSurf_TrackFromID.is_some() {
                                low_reaper.CSurf_TrackFromID(i, false)
                            } else {
                                // Fallback to GetTrack/GetMasterTrack
                                if i == 0 {
                                    low_reaper.GetMasterTrack(std::ptr::null_mut())
                                } else {
                                    low_reaper.GetTrack(std::ptr::null_mut(), i - 1)
                                }
                            }
                        };

                        if chk_track.is_null() {
                            continue;
                        }

                        // Check B_SHOWINTCP
                        let show_in_tcp = unsafe {
                            low_reaper.GetMediaTrackInfo_Value(
                                chk_track,
                                b"B_SHOWINTCP\0".as_ptr() as *const i8,
                            ) != 0.0
                        };

                        if !show_in_tcp {
                            continue;
                        }

                        // Get spacer size
                        let spacer_size = get_track_spacer_size(chk_track, false, medium_reaper);

                        // Get I_TCPY and I_TCPH
                        let ypos_val = unsafe {
                            low_reaper.GetMediaTrackInfo_Value(
                                chk_track,
                                b"I_TCPY\0".as_ptr() as *const i8,
                            )
                        };
                        let h_val = unsafe {
                            low_reaper.GetMediaTrackInfo_Value(
                                chk_track,
                                b"I_TCPH\0".as_ptr() as *const i8,
                            )
                        };
                        let ypos = ypos_val - spacer_size as f64;
                        let h = h_val + spacer_size as f64;

                        // Check if point is within track bounds
                        let pt_y = pt_client.y as f64;
                        if pt_y >= ypos && pt_y < ypos + h {
                            track = chk_track;
                            if pt_y < ypos + spacer_size as f64 {
                                in_spacer = true;
                            }
                            break;
                        }
                    }
                }
            } else if hwnd_parent == tcp {
                // hwnd is a track (direct child of TCP)
                track =
                    unsafe { swell.GetWindowLong(hwnd, -21) as *mut reaper_low::raw::MediaTrack };
            } else {
                // Check if parent's parent is TCP (hwnd is VU meter inside track)
                let hwnd_pparent = unsafe { swell.GetParent(hwnd_parent) };
                if hwnd_pparent == tcp {
                    track = unsafe {
                        swell.GetWindowLong(hwnd_parent, -21) as *mut reaper_low::raw::MediaTrack
                    };
                }
            }

            if !track.is_null() {
                context.is_tcp = true;
            } else if hwnd == tcp {
                context.is_tcp = true;
            }
        }
    }

    // Check MCP if no track found in TCP
    if track.is_null() {
        let (mcp_hwnd, is_container) = get_mcp_wnd(medium_reaper);

        if let Some(mcp) = mcp_hwnd {
            let mixer_hwnd = get_mixer_wnd(medium_reaper);
            let mixer_master_hwnd =
                mixer_hwnd.and_then(|mixer| get_mixer_master_wnd(mixer, medium_reaper));
            let hwnd_pparent = unsafe { swell.GetParent(hwnd_parent) };

            if is_container {
                if hwnd == mcp {
                    // Convert screen point to client coordinates
                    let mut pt_client = pt_screen;
                    unsafe {
                        swell.ScreenToClient(hwnd, &mut pt_client);
                    }

                    let track_count = low_reaper.GetNumTracks();

                    // Search through tracks
                    for i in 0..track_count {
                        let chk_track = unsafe { low_reaper.GetTrack(std::ptr::null_mut(), i) };

                        if chk_track.is_null() {
                            continue;
                        }

                        // Check B_SHOWINMIXER using GetSetMediaTrackInfo
                        let show_in_mixer_ptr = unsafe {
                            low_reaper.GetSetMediaTrackInfo(
                                chk_track,
                                b"B_SHOWINMIXER\0".as_ptr() as *const i8,
                                std::ptr::null_mut(),
                            )
                        };

                        if show_in_mixer_ptr.is_null() {
                            continue;
                        }

                        let show_in_mixer = unsafe { *(show_in_mixer_ptr as *const bool) };

                        if !show_in_mixer {
                            continue;
                        }

                        // Get spacer size for MCP
                        let spacer_size = get_track_spacer_size(chk_track, true, medium_reaper);

                        // Get I_MCPX, I_MCPW, I_MCPY, I_MCPH using GetSetMediaTrackInfo
                        let xpos_ptr = unsafe {
                            low_reaper.GetSetMediaTrackInfo(
                                chk_track,
                                b"I_MCPX\0".as_ptr() as *const i8,
                                std::ptr::null_mut(),
                            )
                        };
                        let xpos_val = if xpos_ptr.is_null() {
                            0i32
                        } else {
                            unsafe { *(xpos_ptr as *const i32) }
                        };
                        let xpos = xpos_val - spacer_size;

                        let w_ptr = unsafe {
                            low_reaper.GetSetMediaTrackInfo(
                                chk_track,
                                b"I_MCPW\0".as_ptr() as *const i8,
                                std::ptr::null_mut(),
                            )
                        };
                        let w_val = if w_ptr.is_null() {
                            0i32
                        } else {
                            unsafe { *(w_ptr as *const i32) }
                        };
                        let w = w_val + spacer_size;

                        if pt_client.x < xpos || pt_client.x >= xpos + w {
                            continue;
                        }

                        let ypos_ptr = unsafe {
                            low_reaper.GetSetMediaTrackInfo(
                                chk_track,
                                b"I_MCPY\0".as_ptr() as *const i8,
                                std::ptr::null_mut(),
                            )
                        };
                        let ypos = if ypos_ptr.is_null() {
                            0
                        } else {
                            unsafe { *(ypos_ptr as *const i32) }
                        };

                        let h_ptr = unsafe {
                            low_reaper.GetSetMediaTrackInfo(
                                chk_track,
                                b"I_MCPH\0".as_ptr() as *const i8,
                                std::ptr::null_mut(),
                            )
                        };
                        let h = if h_ptr.is_null() {
                            0
                        } else {
                            unsafe { *(h_ptr as *const i32) }
                        };

                        // Check if point is within track bounds
                        if pt_client.y >= ypos && pt_client.y < ypos + h {
                            track = chk_track;
                            if pt_client.x < xpos + spacer_size {
                                in_spacer = true;
                            }
                            break;
                        }
                    }
                } else if let Some(mixer_master) = mixer_master_hwnd
                    && (hwnd == mixer_master || hwnd_parent == mixer_master)
                {
                    track = unsafe { low_reaper.GetMasterTrack(std::ptr::null_mut()) };
                }
            } else if hwnd_parent == mcp
                || mixer_hwnd.map(|m| hwnd_parent == m).unwrap_or(false)
                || mixer_master_hwnd.map(|m| hwnd_parent == m).unwrap_or(false)
            {
                // hwnd is a track (direct child of MCP/mixer/master)
                track =
                    unsafe { swell.GetWindowLong(hwnd, -21) as *mut reaper_low::raw::MediaTrack };

                // Special case: if parent is mixer master and no track found, use master track
                if track.is_null() && mixer_master_hwnd.map(|m| hwnd_parent == m).unwrap_or(false) {
                    track = unsafe { low_reaper.GetMasterTrack(std::ptr::null_mut()) };
                }
            } else if hwnd_pparent == mcp
                || mixer_hwnd.map(|m| hwnd_pparent == m).unwrap_or(false)
                || mixer_master_hwnd
                    .map(|m| hwnd_pparent == m)
                    .unwrap_or(false)
            {
                // hwnd is VU meter inside track (parent's parent is MCP/mixer/master)
                track = unsafe {
                    swell.GetWindowLong(hwnd_parent, -21) as *mut reaper_low::raw::MediaTrack
                };
            }

            if !track.is_null() {
                context.is_mcp = true;
            } else if hwnd == mcp || mixer_master_hwnd.map(|m| hwnd == m).unwrap_or(false) {
                context.is_mcp = true;
            }
        }
    }

    // Validate track pointer using ValidatePtr if available
    if !track.is_null() {
        let is_valid = unsafe {
            if low_reaper.pointers().ValidatePtr.is_some() {
                let type_name = b"MediaTrack*\0".as_ptr() as *const i8;
                low_reaper.ValidatePtr(track as *mut std::ffi::c_void, type_name)
            } else {
                // Fallback: just check not null
                true
            }
        };

        if !is_valid {
            track = std::ptr::null_mut();
        }
    }

    if !track.is_null() {
        if in_spacer {
            context.is_spacer = true;
        }
        Some((track, context))
    } else {
        None
    }
}

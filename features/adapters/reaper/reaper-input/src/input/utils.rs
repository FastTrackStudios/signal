//! REAPER Utilities
//!
//! Port of functions from BR_Util.cpp (SWS).
//! Provides utilities for working with tracks, items, takes, and windows.

use crate::input::constants::{
    TAKE_MIN_HEIGHT_COUNT, TAKE_MIN_HEIGHT_HIGH, TAKE_MIN_HEIGHT_LOW, TCP_MASTER_GAP,
};
use reaper_low::raw::*;
use reaper_medium::Reaper as MediumReaper;

// Constants are now imported from crate::input::constants

/// Get take height and offset
/// Port of BR_Util::GetTakeHeight(MediaItem_Take* take, int* offsetY)
pub fn get_take_height(
    take: *mut MediaItem_Take,
    offset_y: &mut i32,
    medium_reaper: &MediumReaper,
) -> i32 {
    get_take_height_full(Some(take), None, -1, offset_y, false, medium_reaper)
}

/// Get take height for a specific take ID
/// Port of BR_Util::GetTakeHeight(MediaItem* item, int id, int* offsetY)
pub fn get_take_height_by_id(
    item: *mut MediaItem,
    take_id: i32,
    offset_y: &mut i32,
    medium_reaper: &MediumReaper,
) -> i32 {
    get_take_height_full(None, Some(item), take_id, offset_y, false, medium_reaper)
}

/// Full implementation of GetTakeHeight
/// Port of BR_Util::GetTakeHeight(MediaItem_Take* take, MediaItem* item, int id, int* offsetY, bool averagedLast)
fn get_take_height_full(
    take: Option<*mut MediaItem_Take>,
    item: Option<*mut MediaItem>,
    take_id: i32,
    offset_y: &mut i32,
    averaged_last: bool,
    medium_reaper: &MediumReaper,
) -> i32 {
    let low_reaper = medium_reaper.low();

    // Validate inputs
    if take.is_none() && item.is_none() {
        *offset_y = 0;
        return 0;
    }

    if take.is_some() && item.is_some() {
        *offset_y = 0;
        return 0;
    }

    // Get valid take and item
    let valid_take = take.unwrap_or_else(|| {
        if let Some(item_ptr) = item {
            unsafe { low_reaper.GetTake(item_ptr, take_id.max(0)) }
        } else {
            std::ptr::null_mut()
        }
    });

    let valid_item = item.unwrap_or_else(|| {
        if !valid_take.is_null() {
            unsafe { low_reaper.GetMediaItemTake_Item(valid_take) }
        } else {
            std::ptr::null_mut()
        }
    });

    if valid_item.is_null() {
        *offset_y = 0;
        return 0;
    }

    // Get track
    let track = unsafe { low_reaper.GetMediaItem_Track(valid_item) };

    if track.is_null() {
        *offset_y = 0;
        return 0;
    }

    // Get track height and offset
    let (track_height, track_offset) = get_track_height_with_spacer(track, medium_reaper);

    // Get item height
    let (item_height, item_offset) = get_item_height(valid_item, medium_reaper);

    // Calculate take offset
    let mut take_offset = track_offset;

    // Get take lanes setting
    // projtakelane is a config var - we'll use a default if not available
    let take_lanes = 1; // Default: take lanes displayed (bit 0 set)

    let mut take_height;

    // Take lanes displayed
    if (take_lanes & 1) != 0 {
        let (effective_take_id, effective_take_count) =
            get_effective_take_id(take, item, take_id, medium_reaper);

        take_height = item_height / effective_take_count.max(1); // average take height

        let min_take_height = get_min_take_height(
            track,
            effective_take_count,
            track_height,
            item_height,
            medium_reaper,
        );

        if take_height < min_take_height {
            let active_take = unsafe { low_reaper.GetActiveTake(valid_item) };
            if active_take == valid_take {
                take_height = item_height;
            } else {
                take_height = 0;
            }
        } else {
            if effective_take_id >= 0 {
                take_offset = item_offset + take_height * effective_take_id;
            }

            // Last take gets leftover pixels from integer division
            if !averaged_last && effective_take_id == effective_take_count - 1 {
                take_height =
                    adjust_last_take_height(item_height, take_height, effective_take_count);
            }
        }
    } else {
        // Take lanes not displayed
        let active_take = unsafe { low_reaper.GetActiveTake(valid_item) };
        if active_take == valid_take {
            take_height = item_height;
        } else {
            take_height = 0;
        }
    }

    *offset_y = take_offset;
    take_height
}

/// Get track height
/// Port of BR_Util::GetTrackHeight()
/// Returns (height, offsetY, topGap, bottomGap)
fn get_track_height(track: *mut MediaTrack, medium_reaper: &MediumReaper) -> (i32, i32, i32, i32) {
    let low_reaper = medium_reaper.low();

    // Get track height from track info
    // I_WNDH is the window height in pixels
    let height_ptr = unsafe {
        low_reaper.GetSetMediaTrackInfo(
            track,
            b"I_WNDH\0".as_ptr() as *const i8,
            std::ptr::null_mut(),
        )
    };

    let height = if height_ptr.is_null() {
        0i32
    } else {
        unsafe { *(height_ptr as *const i32) }
    };

    // Get I_TCPY for offsetY
    let offset_y = unsafe {
        low_reaper.GetMediaTrackInfo_Value(track, b"I_TCPY\0".as_ptr() as *const i8) as i32
    };

    // TODO: Get topGap and bottomGap from track properties
    // These might be in track properties or need to be calculated
    // For now, return 0 for both
    let top_gap = 0;
    let bottom_gap = 0;

    (height.max(0), offset_y, top_gap, bottom_gap)
}

/// Get track height with spacer
/// Port of BR_Util::GetTrackHeightWithSpacer()
/// This is not balanced with SetTrackHeight(), but provides accurate values for v7 tracks with spacers
/// Returns (height, offsetY, topGap, bottomGap)
pub fn get_track_height_with_spacer(
    track: *mut MediaTrack,
    medium_reaper: &MediumReaper,
) -> (i32, i32) {
    let (height, mut offset_y, _top_gap, _bottom_gap) = get_track_height(track, medium_reaper);

    // Get track spacer size
    let track_spacer_size = crate::input::reaper_windows::get_track_spacer_size(
        track,
        false, // is_mcp
        medium_reaper,
    );

    let mut final_height = height;
    if height != 0 {
        final_height += track_spacer_size;
    }

    // Adjust offsetY by subtracting spacer size
    offset_y -= track_spacer_size;

    // Return (height, offsetY) - keeping the simple return for backward compatibility
    // Note: topGap and bottomGap are not returned in the simple version
    (final_height, offset_y)
}

/// Get item height (full version)
/// Port of BR_Util::GetItemHeight(MediaItem* item, int* offsetY, int trackH, int trackOffsetY)
/// This is the internal implementation that does the actual calculation
fn get_item_height_full(
    _item: *mut MediaItem,
    _track_height: i32,
    track_offset: i32,
    medium_reaper: &MediumReaper,
) -> (i32, i32) {
    let _low_reaper = medium_reaper.low();

    // Get item height from item info
    // This is a simplified version - the real implementation is more complex
    // I_CUSTOMCOLOR doesn't give us height, but we can approximate
    // For now, use a default height
    // TODO: Implement proper item height calculation using I_FREEMODE and other factors
    let item_height = 20; // Default item height - this should be calculated properly

    (item_height, track_offset)
}

/// Get item height (simple version)
/// Port of BR_Util::GetItemHeight(MediaItem* item, int* offsetY)
/// Gets track info and calls the full version
pub fn get_item_height(item: *mut MediaItem, medium_reaper: &MediumReaper) -> (i32, i32) {
    let low_reaper = medium_reaper.low();

    // Get the track for this item
    let track = unsafe { low_reaper.GetMediaItem_Track(item) };

    if track.is_null() {
        return (0, 0);
    }

    // Get track height with spacer
    let (track_height, track_offset_y) = get_track_height_with_spacer(track, medium_reaper);

    // Call the full version
    get_item_height_full(item, track_height, track_offset_y, medium_reaper)
}

/// Get effective take ID and count
/// Simplified version
fn get_effective_take_id(
    take: Option<*mut MediaItem_Take>,
    item: Option<*mut MediaItem>,
    take_id: i32,
    medium_reaper: &MediumReaper,
) -> (i32, i32) {
    let low_reaper = medium_reaper.low();

    let valid_item = item.unwrap_or_else(|| {
        if let Some(take_ptr) = take {
            unsafe { low_reaper.GetMediaItemTake_Item(take_ptr) }
        } else {
            std::ptr::null_mut()
        }
    });

    if valid_item.is_null() {
        return (-1, 0);
    }

    // Count takes
    let mut take_count = 0;
    let mut current_take = unsafe { low_reaper.GetTake(valid_item, 0) };

    while !current_take.is_null() {
        take_count += 1;
        current_take = unsafe { low_reaper.GetTake(valid_item, take_count) };
    }

    // Find take ID
    let effective_id = if let Some(take_ptr) = take {
        // Find which take index this is
        for i in 0..take_count {
            let t = unsafe { low_reaper.GetTake(valid_item, i) };
            if t == take_ptr {
                return (i, take_count);
            }
        }
        -1
    } else if take_id >= 0 {
        take_id
    } else {
        -1
    };

    (effective_id, take_count)
}

/// Get minimum take height
fn get_min_take_height(
    _track: *mut MediaTrack,
    take_count: i32,
    _track_height: i32,
    _item_height: i32,
    _medium_reaper: &MediumReaper,
) -> i32 {
    if take_count <= TAKE_MIN_HEIGHT_COUNT {
        TAKE_MIN_HEIGHT_HIGH
    } else {
        TAKE_MIN_HEIGHT_LOW
    }
}

/// Adjust last take height
fn adjust_last_take_height(item_height: i32, take_height: i32, take_count: i32) -> i32 {
    let total_height = take_height * take_count;
    let leftover = item_height - total_height;
    take_height + leftover.max(0)
}

/// Result of GetTrackOrEnvelopeFromY
#[derive(Debug, Clone)]
pub struct TrackOrEnvelopeResult {
    pub envelope: Option<*mut TrackEnvelope>,
    pub track: Option<*mut MediaTrack>,
    pub height: i32,
    pub offset: i32,
    pub spacer_size: i32,
}

/// Get track or envelope from Y coordinate in TCP
/// Port of BR_MouseInfo::GetTrackOrEnvelopeFromY()
///
/// If Y is at track, returns track pointer and all envelopes that have control panels in TCP.
/// If Y is at envelope, returns the envelope and its track.
/// Height and offset are returned for element under Y.
pub fn get_track_or_envelope_from_y(y: i32, medium_reaper: &MediumReaper) -> TrackOrEnvelopeResult {
    let low_reaper = medium_reaper.low();
    let mut result = TrackOrEnvelopeResult {
        envelope: None,
        track: None,
        height: 0,
        offset: 0,
        spacer_size: 0,
    };

    // Get track area from Y
    let (track, element_offset, spacer) = get_track_area_from_y(y, medium_reaper);

    if track.is_null() {
        return result;
    }

    result.track = Some(track);
    result.offset = element_offset;
    result.spacer_size = spacer;

    // Get track height with spacer
    let (track_height, _) = get_track_height_with_spacer(track, medium_reaper);
    result.height = track_height;

    let y_in_track = y < element_offset + track_height;

    // Enumerate envelopes
    let env_count = unsafe { low_reaper.CountTrackEnvelopes(track) };

    for i in 0..env_count {
        let env = unsafe { low_reaper.GetTrackEnvelope(track, i) };

        if env.is_null() {
            continue;
        }

        // Get envelope height
        let env_h = unsafe {
            low_reaper.GetEnvelopeInfo_Value(env, b"I_TCPH\0".as_ptr() as *const i8) as i32
        };

        if env_h < 1 {
            continue;
        }

        // Get envelope Y position
        let env_y = unsafe {
            low_reaper.GetEnvelopeInfo_Value(env, b"I_TCPY\0".as_ptr() as *const i8) as i32
        };

        // Check if envelope has a control panel (envY >= trackHeight - spacer)
        if env_y < track_height - spacer {
            continue;
        }

        let env_y_absolute = env_y + element_offset;

        if y_in_track {
            // If Y is in track area, we don't set envelope but could collect all envelopes here
            // For now, we just continue
        } else if y >= env_y_absolute && y < env_y_absolute + env_h {
            // Y is in this envelope
            result.envelope = Some(env);

            // Get used height and Y (without AI label gap)
            let env_y_used = unsafe {
                low_reaper.GetEnvelopeInfo_Value(env, b"I_TCPY_USED\0".as_ptr() as *const i8) as i32
            };
            let env_h_used = unsafe {
                low_reaper.GetEnvelopeInfo_Value(env, b"I_TCPH_USED\0".as_ptr() as *const i8) as i32
            };

            result.offset = element_offset + env_y_used;
            result.height = env_h_used;
            break;
        }
    }

    result
}

/// Get track area from Y coordinate
/// Port of BR_Util::GetTrackAreaFromY()
///
/// Check if Y is in some TCP track or its envelopes.
/// Returns (track, offset, spacer_size)
fn get_track_area_from_y(y: i32, medium_reaper: &MediumReaper) -> (*mut MediaTrack, i32, i32) {
    let low_reaper = medium_reaper.low();
    let master = unsafe { low_reaper.GetMasterTrack(std::ptr::null_mut()) };

    let mut track: *mut MediaTrack = std::ptr::null_mut();
    let mut track_end = 0;
    let mut track_offset = 0;
    let mut track_spacer_size = 0;

    let track_count = low_reaper.GetNumTracks();

    // Iterate through tracks (including master at i=0)
    for i in 0..=track_count {
        let current_track = unsafe {
            // Use CSurf_TrackFromID if available
            if low_reaper.pointers().CSurf_TrackFromID.is_some() {
                low_reaper.CSurf_TrackFromID(i, false)
            } else {
                // Fallback
                if i == 0 {
                    low_reaper.GetMasterTrack(std::ptr::null_mut())
                } else {
                    low_reaper.GetTrack(std::ptr::null_mut(), i - 1)
                }
            }
        };

        if current_track.is_null() {
            continue;
        }

        // Get track height (I_WNDH)
        let height_ptr = unsafe {
            low_reaper.GetSetMediaTrackInfo(
                current_track,
                b"I_WNDH\0".as_ptr() as *const i8,
                std::ptr::null_mut(),
            )
        };

        let mut height = if height_ptr.is_null() {
            0i32
        } else {
            unsafe { *(height_ptr as *const i32) }
        };

        // Get spacer size
        track_spacer_size = crate::input::reaper_windows::get_track_spacer_size(
            current_track,
            false,
            medium_reaper,
        );
        height += track_spacer_size;

        // Add master TCP gap if this is master track and visible
        if current_track == master {
            let master_vis = low_reaper.GetMasterTrackVisibility();
            if (master_vis & 1) != 0 {
                // Master is visible in TCP - add master TCP gap
                height += TCP_MASTER_GAP;
            }
        }

        track_end += height;

        if y >= track_offset && y < track_end {
            track = current_track;
            break;
        }

        track_offset += height;
        track_spacer_size = 0;
    }

    (track, track_offset, track_spacer_size)
}

/// Get track from Y coordinate (more specific than GetTrackAreaFromY)
/// Port of BR_Util::GetTrackFromY()
///
/// Returns (track, track_height, offset, spacer_size)
pub fn get_track_from_y(
    y: i32,
    medium_reaper: &MediumReaper,
) -> (Option<*mut MediaTrack>, i32, i32, i32) {
    let (track, track_offset, spacer) = get_track_area_from_y(y, medium_reaper);

    if track.is_null() {
        return (None, 0, 0, 0);
    }

    let (track_h, _) = get_track_height_with_spacer(track, medium_reaper);

    // Check if Y is actually within track bounds (not in envelope area)
    if y < track_offset || y >= track_offset + track_h {
        return (None, 0, 0, 0);
    }

    (
        Some(track),
        track_h,
        track_offset,
        if spacer != 0 { spacer } else { 0 },
    )
}

/// Check if point is in arrange view
/// Port of BR_Util::IsPointInArrange()
///
/// Since WindowFromPoint is sometimes called before this function,
/// caller can choose not to check it but only if point's coordinates
/// are in arrange (disregarding scrollbars).
pub fn is_point_in_arrange(
    p: POINT,
    check_point_visibility: bool,
    medium_reaper: &MediumReaper,
) -> (bool, Option<HWND>) {
    use reaper_low::Swell;

    use reaper_low::raw::RECT;

    let arrange_hwnd = match crate::input::reaper_windows::get_arrange_wnd(medium_reaper) {
        Some(hwnd) => hwnd,
        None => return (false, None),
    };

    let swell = Swell::get();

    // Get window rect
    let mut r = RECT {
        left: 0,
        right: 0,
        top: 0,
        bottom: 0,
    };
    unsafe {
        swell.GetWindowRect(arrange_hwnd, &mut r);
    }

    // Adjust for scrollbar width
    const SCROLLBAR_W: i32 = 16; // Approximate scrollbar width
    #[cfg(target_os = "windows")]
    {
        r.right -= SCROLLBAR_W + 1;
        r.bottom -= SCROLLBAR_W + 1;
    }
    #[cfg(not(target_os = "windows"))]
    {
        r.right -= SCROLLBAR_W;
        if r.top > r.bottom {
            // Swap if needed (macOS/Linux)
            std::mem::swap(&mut r.top, &mut r.bottom);
            r.top += SCROLLBAR_W;
        } else {
            r.bottom -= SCROLLBAR_W;
        }
    }

    let point_in_arrange = p.x >= r.left && p.x <= r.right && p.y >= r.top && p.y <= r.bottom;

    if check_point_visibility {
        if point_in_arrange {
            let hwnd_pt = unsafe { swell.WindowFromPoint(p) };
            if arrange_hwnd == hwnd_pt {
                (true, Some(hwnd_pt))
            } else {
                (false, Some(hwnd_pt))
            }
        } else {
            let hwnd_pt = unsafe { swell.WindowFromPoint(p) };
            (false, Some(hwnd_pt))
        }
    } else {
        let hwnd_pt = unsafe { swell.WindowFromPoint(p) };
        (point_in_arrange, Some(hwnd_pt))
    }
}

/// Get position at arrange point
/// Port of BR_Util::PositionAtArrangePoint()
///
/// Does not check if point is in arrange. To make it work, it's enough
/// to have p.x in arrange, p.y can be somewhere else like ruler etc.
///
/// Returns (position, arrange_start, arrange_end, h_zoom)
pub fn position_at_arrange_point(p: POINT, medium_reaper: &MediumReaper) -> (f64, f64, f64, f64) {
    use reaper_low::Swell;

    use reaper_low::raw::RECT;

    let arrange_hwnd = match crate::input::reaper_windows::get_arrange_wnd(medium_reaper) {
        Some(hwnd) => hwnd,
        None => return (0.0, 0.0, 0.0, 1.0),
    };

    let swell = Swell::get();
    let low_reaper = medium_reaper.low();

    // Convert to client coordinates
    let mut pt_client = p;
    unsafe {
        swell.ScreenToClient(arrange_hwnd, &mut pt_client);
    }

    // Get window rect
    let mut r = RECT {
        left: 0,
        right: 0,
        top: 0,
        bottom: 0,
    };
    unsafe {
        swell.GetWindowRect(arrange_hwnd, &mut r);
    }

    // Get arrange view bounds
    let mut start = 0.0;
    let mut end = 0.0;
    unsafe {
        low_reaper.GetSet_ArrangeView2(
            std::ptr::null_mut(), // project (NULL = current)
            false,                // isSet
            r.left,               // screen_x_start
            r.right - 16,         // screen_x_end (subtract scrollbar width)
            &mut start,
            &mut end,
        );
    }

    // Get client rect
    let mut client_r = RECT {
        left: 0,
        right: 0,
        top: 0,
        bottom: 0,
    };
    unsafe {
        swell.GetClientRect(arrange_hwnd, &mut client_r);
    }

    // Get horizontal zoom level
    let zoom = low_reaper.GetHZoomLevel();

    // Calculate position
    let position = start + (client_r.left + pt_client.x) as f64 / zoom;

    (position, start, end, zoom)
}

/// Translate point to arrange scroll Y
/// Port of BR_Util::TranslatePointToArrangeScrollY()
///
/// Returns the Y coordinate in arrange view scroll space.
pub fn translate_point_to_arrange_scroll_y(p: POINT, medium_reaper: &MediumReaper) -> i32 {
    use reaper_low::Swell;

    let arrange_hwnd = match crate::input::reaper_windows::get_arrange_wnd(medium_reaper) {
        Some(hwnd) => hwnd,
        None => return 0,
    };

    let swell = Swell::get();
    let low_reaper = medium_reaper.low();

    // Convert to client coordinates
    let mut pt_client = p;
    unsafe {
        swell.ScreenToClient(arrange_hwnd, &mut pt_client);
    }

    // Get scroll position using CoolSB_GetScrollInfo (REAPER's version of CF_GetScrollInfo)
    // SB_VERT = 1 for vertical scrollbar
    const SB_VERT: i32 = 1;
    const SIF_POS: u32 = 0x0004; // Request scroll position

    // SCROLLINFO structure (Windows API)
    #[repr(C)]
    struct ScrollInfo {
        cb_size: u32,
        f_mask: u32,
        n_min: i32,
        n_max: i32,
        n_page: u32,
        n_pos: i32,
        n_track_pos: i32,
    }

    // Initialize SCROLLINFO like SWS does: { sizeof(SCROLLINFO), SIF_POS }
    let mut scroll_info = ScrollInfo {
        cb_size: std::mem::size_of::<ScrollInfo>() as u32,
        f_mask: SIF_POS,
        n_min: 0,
        n_max: 0,
        n_page: 0,
        n_pos: 0,
        n_track_pos: 0,
    };

    // Get scroll position using CoolSB_GetScrollInfo (REAPER's equivalent to CF_GetScrollInfo)
    if low_reaper.pointers().CoolSB_GetScrollInfo.is_some() {
        unsafe {
            let scroll_info_ptr = &mut scroll_info as *mut ScrollInfo;
            let _result =
                low_reaper.CoolSB_GetScrollInfo(arrange_hwnd, SB_VERT, scroll_info_ptr as *mut _);
            // Return client Y + scroll position (matching SWS: return (int)p.y + si.nPos)
            return pt_client.y + scroll_info.n_pos;
        }
    }

    // Fallback: return client Y coordinate if scroll info not available
    pt_client.y
}

// Re-export reaper_windows functions for backward compatibility
pub use crate::input::reaper_windows::{get_arrange_wnd, get_ruler_wnd, get_transport_wnd};

//! Mouse-cursor context — "what is currently under the mouse?"
//!
//! Modelled after SWS's `BR_GetMouseCursorContext`: a single
//! [`MouseSnapshot::capture`] call hit-tests the cursor once and bundles
//! everything we know about that point into one struct. Accessors are
//! plain field reads so callers can pull a `track`, an `item`, a
//! `project_position`, etc. without each one re-querying REAPER.
//!
//! v1 scope (Arrange + Ruler + TCP/MCP basics):
//! - Window classification (Arrange / Tcp / Mcp / Ruler / MidiEditor / Transport / Unknown)
//! - Segment classification within Arrange (Track / Empty)
//! - Detail (`Item` when an item is under the mouse in arrange)
//! - `project_position` for Arrange / Ruler hover
//! - `track` / `item` / `take` / `marker_index` / `region_index`
//!
//! Not yet covered (extend as needed): envelopes, stretch markers,
//! MIDI-editor fields, automation items, sub-lane classification in
//! the ruler beyond marker vs region.

use reaper_low::{Swell, raw};
use reaper_medium::{MediaItem, MediaItemTake, MediaTrack};

/// Which REAPER window the mouse is over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseWindow {
    Unknown,
    Arrange,
    Tcp,
    Mcp,
    Ruler,
    MidiEditor,
    Transport,
}

/// Sub-region within the window (mirrors SWS `segment`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseSegment {
    Unknown,
    /// Arrange: cursor is over a track lane (item-bearing region).
    Track,
    /// Arrange: cursor is in empty arrange space (below all tracks).
    Empty,
    /// Ruler: marker lane (point markers).
    MarkerLane,
    /// Ruler: region lane.
    RegionLane,
    /// Ruler / timeline area without a marker/region underneath.
    Timeline,
}

/// Finer-grained detail (mirrors SWS `details`). Currently only used
/// for Arrange/Track — more variants may be added later.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseDetails {
    Unknown,
    Empty,
    Item,
}

/// One-shot snapshot of "what's under the mouse right now". Always
/// constructed via [`MouseSnapshot::capture`]; never mutate after that.
///
/// Every "resolved thing" field is `Option` — present only when the
/// snapshot actually located one. Code that needs e.g. a take just
/// does `let Some(take) = m.take else { return };` without re-querying.
#[derive(Debug)]
pub struct MouseSnapshot {
    /// Screen-space mouse position at capture time.
    pub screen_pos: raw::POINT,
    pub window: MouseWindow,
    pub segment: MouseSegment,
    pub details: MouseDetails,
    /// Project-timeline position under the mouse (seconds). Set when
    /// the mouse is over Arrange or Ruler and the arrange-view bounds
    /// resolve cleanly; `None` otherwise.
    pub project_position: Option<f64>,
    pub track: Option<MediaTrack>,
    pub item: Option<MediaItem>,
    pub take: Option<MediaItemTake>,
    /// Marker index (REAPER's enumeration order) for a point marker the
    /// mouse is sitting on in the ruler — `None` when over a region or
    /// no marker at all.
    pub marker_index: Option<usize>,
    /// Region index for a region containing `project_position`.
    pub region_index: Option<usize>,
}

impl MouseSnapshot {
    /// Hit-test the cursor's current screen position and bundle
    /// everything we resolve into a snapshot. Must run on REAPER's main
    /// thread.
    pub fn capture() -> Self {
        let low = reaper_low::Reaper::get();

        let mut screen_pos = raw::POINT { x: 0, y: 0 };
        unsafe { low.GetMousePosition(&mut screen_pos.x, &mut screen_pos.y) };

        let mut snap = Self {
            screen_pos,
            window: MouseWindow::Unknown,
            segment: MouseSegment::Unknown,
            details: MouseDetails::Unknown,
            project_position: None,
            track: None,
            item: None,
            take: None,
            marker_index: None,
            region_index: None,
        };

        // Classify the window via WindowFromPoint + comparison to known
        // REAPER HWNDs. The arrange window doesn't have a typed helper
        // in reaper-medium, so we rely on `GetItemFromPoint` /
        // `GetTrackFromPoint` returning hits to confirm arrange.
        let hwnd_at_point = unsafe { Swell::get().WindowFromPoint(screen_pos) };
        let main_hwnd = unsafe { low.GetMainHwnd() };

        // ── Item / Take (Arrange) ────────────────────────────────────────
        let mut take_out: *mut raw::MediaItem_Take = std::ptr::null_mut();
        let item_ptr =
            unsafe { low.GetItemFromPoint(screen_pos.x, screen_pos.y, false, &mut take_out) };
        if !item_ptr.is_null() {
            snap.item = MediaItem::new(item_ptr);
            snap.take = MediaItemTake::new(take_out);
            snap.window = MouseWindow::Arrange;
            snap.segment = MouseSegment::Track;
            snap.details = MouseDetails::Item;
        }

        // ── Track (Arrange or TCP) ───────────────────────────────────────
        // GetTrackFromPoint returns a track for both arrange-lane hover
        // and TCP/MCP hover. `info_out` distinguishes via bit flags but
        // varies by REAPER version; for v1 we just record the track and
        // use prior window classification.
        let mut info_out: i32 = 0;
        let track_ptr = unsafe { low.GetTrackFromPoint(screen_pos.x, screen_pos.y, &mut info_out) };
        if !track_ptr.is_null() {
            snap.track = MediaTrack::new(track_ptr);
            if snap.window == MouseWindow::Unknown {
                // Track hit but no item — could be empty arrange lane
                // or TCP. Default to Arrange/Empty; refine below if the
                // hwnd looks like TCP.
                snap.window = MouseWindow::Arrange;
                snap.segment = MouseSegment::Track;
                snap.details = MouseDetails::Empty;
            }
        }

        // ── Project position (Arrange) ───────────────────────────────────
        if matches!(snap.window, MouseWindow::Arrange) {
            snap.project_position = project_pos_for_arrange(low, main_hwnd, screen_pos);
        }

        // ── Ruler (markers + regions) ────────────────────────────────────
        // SWS does deeper classification here; for v1 we use a simple
        // signal: if we couldn't classify the window as Arrange AND the
        // hwnd-at-point isn't null AND it isn't main, treat it as Ruler
        // and try to resolve project position the same way (the math is
        // identical for the timeline strip immediately above arrange).
        if matches!(snap.window, MouseWindow::Unknown) && !hwnd_at_point.is_null() {
            // Best-effort: same project-time math (the ruler shares the
            // arrange's horizontal projection). If it resolves, classify
            // as Ruler/Timeline and look up containing marker/region.
            if let Some(pos) = project_pos_for_arrange(low, main_hwnd, screen_pos) {
                snap.window = MouseWindow::Ruler;
                snap.segment = MouseSegment::Timeline;
                snap.project_position = Some(pos);
                let (mi, ri) = marker_and_region_at(low, pos);
                snap.marker_index = mi;
                snap.region_index = ri;
            }
        } else if let Some(pos) = snap.project_position {
            // We're in arrange, but also surface region containing pos —
            // useful for "do X to the current region" actions even when
            // the mouse is over an item rather than the ruler.
            let (_mi, ri) = marker_and_region_at(low, pos);
            snap.region_index = ri;
        }

        snap
    }

    /// Convenience: source-time position on `self.take` corresponding to
    /// `self.project_position`, accounting for item start / take
    /// play-rate / take start-offset. `None` if any required field is
    /// missing or the resulting source position would be negative.
    pub fn take_source_position(&self) -> Option<f64> {
        let take = self.take?;
        let item = self.item?;
        let proj = self.project_position?;
        let low = reaper_low::Reaper::get();
        unsafe {
            let item_start = low.GetMediaItemInfo_Value(item.as_ptr(), c_str_d_position());
            let play_rate = low.GetMediaItemTakeInfo_Value(take.as_ptr(), c_str_d_playrate());
            let start_offset = low.GetMediaItemTakeInfo_Value(take.as_ptr(), c_str_d_startoffs());
            let src = (proj - item_start) * play_rate + start_offset;
            if src < 0.0 { None } else { Some(src) }
        }
    }

    /// Braced GUID of the item under the mouse (matches `Items`/`ItemRef`
    /// GUID format), if any.
    pub fn item_guid(&self) -> Option<String> {
        let item = self.item?;
        let medium = reaper_high::Reaper::get().medium_reaper();
        Some(crate::item::item_guid_string(medium, item))
    }

    /// GUID of the take under the mouse (matches `Takes`/`TakeRef` GUID
    /// format), if any.
    pub fn take_guid(&self) -> Option<String> {
        let take = self.take?;
        let low = reaper_low::Reaper::get();
        Some(crate::safe_wrappers::item::get_take_guid_string(low, take))
    }
}

// ─── Helpers ────────────────────────────────────────────────────────────────

/// REAPER's arrange-view child window carries the well-known dialog
/// control ID `0x3E8` ("trackview") under the main HWND. This matches
/// SWS's `GetArrangeWnd()` helper (see SWS `BR_Util.cpp`). The SDK
/// exposes no typed accessor for the arrange HWND, so the control ID
/// is the canonical way to find it across REAPER versions / platforms.
const ARRANGE_DLG_CTL_ID: i32 = 0x3E8;

fn arrange_hwnd(main_hwnd: raw::HWND) -> Option<raw::HWND> {
    let hwnd = unsafe { Swell::get().GetDlgItem(main_hwnd, ARRANGE_DLG_CTL_ID) };
    if hwnd.is_null() { None } else { Some(hwnd) }
}

fn project_pos_for_arrange(
    low: &reaper_low::Reaper,
    main_hwnd: raw::HWND,
    screen_pos: raw::POINT,
) -> Option<f64> {
    // Use the arrange CHILD window (not main) for client-coord
    // conversion — main's x=0 is the left edge of the whole REAPER
    // window (before the TCP), arrange's x=0 is the actual track-view
    // origin. Mixing them up caused our previous offset bug.
    let arrange = arrange_hwnd(main_hwnd)?;

    let mut win_rect = raw::RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    unsafe { Swell::get().GetWindowRect(arrange, &mut win_rect) };

    // Ask REAPER for the time range visible at the arrange's actual
    // screen-x extent, the way SWS does it. Passing 0,0 here returned
    // values that didn't anchor to the arrange's true left edge.
    let mut start_time: f64 = 0.0;
    let mut end_time: f64 = 0.0;
    unsafe {
        low.GetSet_ArrangeView2(
            std::ptr::null_mut(),
            false,
            win_rect.left,
            win_rect.right,
            &mut start_time,
            &mut end_time,
        );
    }
    let h_zoom = low.GetHZoomLevel();
    if h_zoom <= 0.0 {
        return None;
    }

    let mut pt = screen_pos;
    unsafe { Swell::get().ScreenToClient(arrange, &mut pt) };
    let pos = start_time + (pt.x as f64) / h_zoom;
    if pos < -10.0 {
        None
    } else {
        Some(pos.max(0.0))
    }
}

/// Walk REAPER's project marker/region enumeration; return the first
/// point marker exactly *at* `pos` (within 1/zoom worth of slop is left
/// to the caller) and the first region whose `[pos_start, pos_end)`
/// contains `pos`. Indices are REAPER's enumeration order (0-based).
fn marker_and_region_at(low: &reaper_low::Reaper, pos: f64) -> (Option<usize>, Option<usize>) {
    let mut marker = None;
    let mut region = None;
    let mut idx: i32 = 0;
    let mut next: i32;
    loop {
        let mut is_rgn = false;
        let mut p_start: f64 = 0.0;
        let mut p_end: f64 = 0.0;
        let mut name_ptr: *const std::os::raw::c_char = std::ptr::null();
        let mut number: i32 = 0;
        next = unsafe {
            low.EnumProjectMarkers2(
                std::ptr::null_mut(),
                idx,
                &mut is_rgn,
                &mut p_start,
                &mut p_end,
                &mut name_ptr,
                &mut number,
            )
        };
        if next <= 0 {
            break;
        }
        let i = idx as usize;
        if is_rgn {
            if region.is_none() && pos >= p_start && pos < p_end {
                region = Some(i);
            }
        } else if marker.is_none() && (pos - p_start).abs() < 1e-6 {
            marker = Some(i);
        }
        idx = next;
        if marker.is_some() && region.is_some() {
            break;
        }
    }
    (marker, region)
}

fn c_str_d_position() -> *const std::os::raw::c_char {
    c"D_POSITION".as_ptr()
}
fn c_str_d_playrate() -> *const std::os::raw::c_char {
    c"D_PLAYRATE".as_ptr()
}
fn c_str_d_startoffs() -> *const std::os::raw::c_char {
    c"D_STARTOFFS".as_ptr()
}

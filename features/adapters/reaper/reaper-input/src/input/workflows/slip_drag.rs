//! FTS-driven slip-edit drag.
//!
//! REAPER's native slip (move item contents) is a *body* drag. When the
//! quick-edit gesture splits the item under the cursor first, the fresh
//! boundary lands exactly under the mouse — so passing the click through makes
//! REAPER read it as an *edge* drag (trim), and no mouse-modifier maps an edge
//! to slip. To get "split then slip the right piece" as one gesture we drive
//! the slip ourselves: on the intercepted mouse-down we split, grab the item to
//! the right of the cursor, capture the mouse, and adjust the take's start
//! offset on every move until the button is released. REAPER never sees the
//! drag, so the edge never matters.
//!
//! All entry points run on REAPER's UI thread (the hooked window proc), so the
//! state lives in a thread-local. Raw item/take pointers are only ever read on
//! that same thread between mouse-down and mouse-up.

use reaper_high::Reaper;
use reaper_low::Swell;
use reaper_low::raw::{HWND, MediaItem, MediaItem_Take, POINT};
use std::cell::RefCell;
use tracing::debug;

/// In-progress slip drag captured on mouse-down.
struct SlipState {
    /// Window that owns the mouse capture (the arrange view).
    hwnd: HWND,
    /// Item being slipped (the split-off right piece).
    item: *mut MediaItem,
    /// Active take whose `D_STARTOFFS` we move.
    take: *mut MediaItem_Take,
    /// Take start offset (source seconds) at gesture start.
    start_offs: f64,
    /// Take play rate — converts project seconds to source seconds.
    playrate: f64,
    /// Horizontal zoom (pixels per project second) at gesture start.
    px_per_sec: f64,
    /// Mouse screen X at gesture start.
    start_x: i32,
}

thread_local! {
    static SLIP: RefCell<Option<SlipState>> = const { RefCell::new(None) };
}

/// True while a slip drag is in progress (mouse-down handled, button not yet up).
pub fn is_active() -> bool {
    SLIP.with(|s| s.borrow().is_some())
}

/// Current mouse position in screen coordinates.
fn cursor_screen() -> POINT {
    let mut pt = POINT { x: 0, y: 0 };
    unsafe {
        Swell::get().GetCursorPos(&mut pt);
    }
    pt
}

/// Begin a slip drag on the item just to the right of the cursor.
///
/// Call *after* the split action has run, so the right piece exists. `screen_x`
/// / `screen_y` are the mouse-down position in screen coordinates. Returns true
/// if a slip target was found and the drag started (mouse captured).
pub fn begin(hwnd: HWND, screen_x: i32, screen_y: i32) -> bool {
    let reaper = Reaper::get();
    let low = reaper.medium_reaper().low();

    // The split put a boundary under the cursor; the right piece's body starts a
    // few pixels to the right. Sample there so we grab the right piece, not the
    // left one (and not the bare boundary).
    let item = super::armed::get_item_at_point(screen_x + 3, screen_y).or_else(|| {
        let sel = unsafe { low.GetSelectedMediaItem(std::ptr::null_mut(), 0) };
        if sel.is_null() { None } else { Some(sel) }
    });
    let Some(item) = item else {
        debug!("slip_drag: no item under cursor after split — not starting");
        return false;
    };

    let take = unsafe { low.GetActiveTake(item) };
    if take.is_null() {
        debug!("slip_drag: item has no active take — not starting");
        return false;
    }

    let start_offs = unsafe { low.GetMediaItemTakeInfo_Value(take, c"D_STARTOFFS".as_ptr()) };
    let mut playrate = unsafe { low.GetMediaItemTakeInfo_Value(take, c"D_PLAYRATE".as_ptr()) };
    if playrate <= 0.0 {
        playrate = 1.0;
    }
    let px_per_sec = reaper.medium_reaper().low().GetHZoomLevel();

    unsafe {
        Swell::get().SetCapture(hwnd);
    }

    SLIP.with(|s| {
        *s.borrow_mut() = Some(SlipState {
            hwnd,
            item,
            take,
            start_offs,
            playrate,
            px_per_sec,
            start_x: screen_x,
        });
    });

    debug!(
        start_offs,
        playrate,
        px_per_sec,
        start_x = screen_x,
        "slip_drag: started"
    );
    true
}

/// Update the slip from the current mouse position. No-op if not active.
pub fn on_move() {
    SLIP.with(|s| {
        let guard = s.borrow();
        let Some(st) = guard.as_ref() else { return };
        if st.px_per_sec <= 0.0 {
            return;
        }

        let dx = cursor_screen().x - st.start_x;
        // Project-time delta the mouse has travelled, then into source seconds.
        let dt_project = dx as f64 / st.px_per_sec;
        let dt_source = dt_project * st.playrate;
        // Drag right → reveal earlier source material at the item start, i.e.
        // the start offset decreases. Clamp at 0 (start of source).
        let new_offs = (st.start_offs - dt_source).max(0.0);

        let low = Reaper::get().medium_reaper().low();
        unsafe {
            low.SetMediaItemTakeInfo_Value(st.take, c"D_STARTOFFS".as_ptr(), new_offs);
            low.UpdateItemInProject(st.item);
            low.UpdateArrange();
        }
    });
}

/// Finish the slip drag: release capture, clear state, mark an undo point.
pub fn on_up() {
    let was_active = SLIP.with(|s| s.borrow_mut().take()).is_some();
    if !was_active {
        return;
    }

    Swell::get().ReleaseCapture();

    let low = Reaper::get().medium_reaper().low();
    unsafe {
        low.Undo_OnStateChange2(std::ptr::null_mut(), c"Quick-edit: split and slip".as_ptr());
        low.UpdateArrange();
    }
    debug!("slip_drag: finished");
}

/// Abort any in-progress slip (e.g. on workflow deactivation). Releases capture
/// without committing an undo label beyond what the moves already applied.
pub fn cancel() {
    let was_active = SLIP.with(|s| s.borrow_mut().take()).is_some();
    if was_active {
        Swell::get().ReleaseCapture();
    }
}

//! `impl StretchMarkers for Reaper` — non-destructive take timing.
//!
//! REAPER's stretch-marker API is six raw functions, and the one thing
//! worth knowing about it is not in the function names: `idx = -1` on
//! `SetTakeStretchMarker` means **add**, and the return value is the
//! index the marker actually landed at, which need not be the last —
//! the host keeps them sorted by position.
//!
//! Slope is a separate call from position, so writing a marker with a
//! curve is always two calls and the index between them has to be the
//! one the host just reported rather than the one that was asked for.

use daw_proto::{
    DawError, DawResult, ItemRef, ProjectContext, StretchMarker, StretchMarkers, StretchMode,
    StretchTakeRef, TakeRef,
};
use reaper_high::Reaper as ReaperHigh;
use reaper_medium::{MediaItemTake, ProjectContext as ReaperProjectContext, TakeAttributeKey};

use crate::item::{ReaperItem, ReaperTake};

/// Resolve a location to a take pointer.
fn take_ptr(item: &ItemRef, take: &TakeRef) -> Option<MediaItemTake> {
    let item_ptr = ReaperItem::resolve_item(item, ReaperProjectContext::CurrentProject)?;
    ReaperTake::resolve_take(item_ptr, take)
}

fn resolve(loc: &StretchTakeRef) -> DawResult<MediaItemTake> {
    take_ptr(&loc.item, &loc.take).ok_or_else(|| DawError::not_found("Take", "location"))
}

/// Write one marker, returning the index the host put it at.
///
/// `-1` adds; anything else replaces. The slope is a second call, and
/// it must use the *returned* index — passing the requested one writes
/// the curve onto whichever marker happens to sit there instead.
unsafe fn write(take: MediaItemTake, idx: i32, marker: &StretchMarker) -> Option<u32> {
    let low = ReaperHigh::get().medium_reaper().low();
    let src = marker.source_position;
    let landed = unsafe {
        low.SetTakeStretchMarker(take.as_ptr(), idx, marker.position, &src as *const f64)
    };
    if landed < 0 {
        return None;
    }
    if marker.slope != 0.0 {
        unsafe {
            low.SetTakeStretchMarkerSlope(take.as_ptr(), landed, marker.slope);
        }
    }
    Some(landed as u32)
}

impl StretchMarkers for crate::Reaper {
    fn get_stretch_markers(
        &self,
        _project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
    ) -> Vec<StretchMarker> {
        let Some(take_ptr) = take_ptr(&item, &take) else {
            return Vec::new();
        };
        let low = ReaperHigh::get().medium_reaper().low();
        let count = unsafe { low.GetTakeNumStretchMarkers(take_ptr.as_ptr()) };
        let mut out = Vec::with_capacity(count.max(0) as usize);
        for i in 0..count {
            let mut pos = 0.0f64;
            let mut src = 0.0f64;
            let ok = unsafe { low.GetTakeStretchMarker(take_ptr.as_ptr(), i, &mut pos, &mut src) };
            if ok < 0 {
                continue;
            }
            let slope = unsafe { low.GetTakeStretchMarkerSlope(take_ptr.as_ptr(), i) };
            out.push(StretchMarker {
                position: pos,
                source_position: src,
                slope,
            });
        }
        out
    }

    fn add_stretch_marker(
        &self,
        location: StretchTakeRef,
        marker: StretchMarker,
    ) -> DawResult<u32> {
        let take = resolve(&location)?;
        unsafe { write(take, -1, &marker) }
            .ok_or_else(|| DawError::internal("REAPER refused the stretch marker position"))
    }

    fn set_stretch_marker(
        &self,
        location: StretchTakeRef,
        index: u32,
        marker: StretchMarker,
    ) -> DawResult<()> {
        let take = resolve(&location)?;
        unsafe { write(take, index as i32, &marker) }
            .map(|_| ())
            .ok_or_else(|| DawError::not_found("StretchMarker", &index.to_string()))
    }

    fn delete_stretch_marker(&self, location: StretchTakeRef, index: u32) -> DawResult<()> {
        let take = resolve(&location)?;
        let low = ReaperHigh::get().medium_reaper().low();
        let removed =
            unsafe { low.DeleteTakeStretchMarkers(take.as_ptr(), index as i32, std::ptr::null()) };
        if removed > 0 {
            Ok(())
        } else {
            Err(DawError::not_found("StretchMarker", &index.to_string()))
        }
    }

    fn clear_stretch_markers(&self, location: StretchTakeRef) -> DawResult<()> {
        let take = resolve(&location)?;
        let low = ReaperHigh::get().medium_reaper().low();
        // Back to front: deleting shifts every later index down, so a
        // forward loop skips every second marker.
        let count = unsafe { low.GetTakeNumStretchMarkers(take.as_ptr()) };
        for i in (0..count).rev() {
            unsafe {
                low.DeleteTakeStretchMarkers(take.as_ptr(), i, std::ptr::null());
            }
        }
        Ok(())
    }

    fn set_stretch_markers(
        &self,
        location: StretchTakeRef,
        markers: Vec<StretchMarker>,
    ) -> DawResult<()> {
        // Clear then write, rather than diffing against what is there.
        // A map is a whole object — half of a new warp over half of an
        // old one is not a timing either of them describes, and the
        // host may play it while the write is in progress.
        self.clear_stretch_markers(location.clone())?;
        let take = resolve(&location)?;
        for m in &markers {
            unsafe { write(take, -1, m) };
        }
        Ok(())
    }

    fn set_stretch_mode(&self, location: StretchTakeRef, mode: StretchMode) -> DawResult<()> {
        let take = resolve(&location)?;
        let medium = ReaperHigh::get().medium_reaper();
        // `I_STRETCHFLAGS` low three bits select the algorithm.
        unsafe {
            medium.set_media_item_take_info_value(
                take,
                TakeAttributeKey::custom("I_STRETCHFLAGS"),
                mode as i32 as f64,
            )
        }
        .map_err(|e| DawError::internal(format!("set stretch mode: {e}")))
    }
}

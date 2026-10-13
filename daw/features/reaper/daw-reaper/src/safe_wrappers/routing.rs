//! Safe wrappers for REAPER routing/send APIs.
//!
//! Most routing operations use the reaper-high `TrackRoute` API which is
//! already safe. These wrappers cover the remaining low-level calls used
//! in `read_send_mode`, `set_send_mode`, and parent-send queries.

use reaper_medium::{
    MediaTrack, SendTarget, TrackAttributeKey, TrackSendAttributeKey, TrackSendCategory,
};

/// Get a track send info value (float).
pub fn get_track_send_info_value(
    medium: &reaper_medium::Reaper,
    track: MediaTrack,
    category: TrackSendCategory,
    index: u32,
    attr: TrackSendAttributeKey,
) -> f64 {
    unsafe { medium.get_track_send_info_value(track, category, index, attr) }
}

/// Set a track send info value (float).
pub fn set_track_send_info_value(
    medium: &reaper_medium::Reaper,
    track: MediaTrack,
    category: TrackSendCategory,
    index: u32,
    attr: TrackSendAttributeKey,
    value: f64,
) {
    unsafe {
        let _ = medium.set_track_send_info_value(track, category, index, attr, value);
    }
}

/// Get a media track info value (float).
pub fn get_media_track_info_value(
    medium: &reaper_medium::Reaper,
    track: MediaTrack,
    attr: TrackAttributeKey,
) -> f64 {
    unsafe { medium.get_media_track_info_value(track, attr) }
}

/// Set a media track info value (float).
pub fn set_media_track_info_value(
    medium: &reaper_medium::Reaper,
    track: MediaTrack,
    attr: TrackAttributeKey,
    value: f64,
) {
    unsafe {
        let _ = medium.set_media_track_info_value(track, attr, value);
    }
}

/// Create a track send (returns send index on success).
pub fn create_track_send(
    medium: &reaper_medium::Reaper,
    track: MediaTrack,
    target: SendTarget,
) -> Result<u32, reaper_medium::ReaperFunctionError> {
    unsafe { medium.create_track_send(track, target) }
}

/// Remove a track send by category and index.
pub fn remove_track_send(
    medium: &reaper_medium::Reaper,
    track: MediaTrack,
    category: TrackSendCategory,
    index: u32,
) {
    unsafe {
        let _ = medium.remove_track_send(track, category, index);
    }
}

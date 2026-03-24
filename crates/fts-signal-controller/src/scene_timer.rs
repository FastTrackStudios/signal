//! Timer-based scene switching on REAPER's main thread.
//!
//! Runs at ~30Hz via the REAPER timer callback registered by
//! `reaper_bootstrap`. Reads the playhead/cursor position and named
//! items on controller tracks to determine which scene should be active,
//! then mutes/unmutes child tracks directly.
//!
//! Items on controller tracks are ordered by position. Each item's take
//! name identifies the scene/song being switched to. The item's sequential
//! index (0-based) maps to the corresponding child track (non-folder
//! children of the controller folder, in track order).
//!
//! Uses reaper-high API directly (no SHM, no async) since we're
//! already on REAPER's main thread.

use daw::reaper::bootstrap::{HighReaper, HighTrack, LowReaper, MediaItem_Take, MediaTrack, ReaperProjectContext};
use std::ffi::CString;
use std::sync::Mutex;
use tracing::{debug, info, warn};

/// P_EXT key for scene_count on controller tracks.
const SCENE_COUNT_KEY: &str = "P_EXT:fts_signal:scene_count";

/// A single entry in the switch timeline.
struct TimelineEntry {
    start: f64,
    end: f64,
    /// 0-based index (sequential item order on the track).
    index: u8,
    /// Take name (e.g. "Clean", "Rhythm", "Belief").
    name: String,
}

/// Cached controller track state.
struct ControllerCache {
    /// REAPER track indices of non-folder child tracks (section/song tracks).
    /// Index 0 = first scene, index 1 = second scene, etc.
    child_track_indices: Vec<u32>,
    /// Name (for logging).
    name: String,
    /// Timeline of named items sorted by position.
    timeline: Vec<TimelineEntry>,
    /// Currently active scene (0-based), -1 = none.
    active_scene: i32,
}

struct SceneState {
    controllers: Vec<ControllerCache>,
    initialized: bool,
    /// Counter to throttle re-scans (re-scan every ~5 seconds = 150 ticks at 30Hz).
    tick_count: u32,
}

static STATE: Mutex<SceneState> = Mutex::new(SceneState {
    controllers: Vec::new(),
    initialized: false,
    tick_count: 0,
});

/// Called from the timer callback at ~30Hz on REAPER's main thread.
pub fn poll() {
    let Ok(mut state) = STATE.try_lock() else {
        return; // Skip if locked (re-scanning)
    };

    state.tick_count += 1;

    // Lazy initialization
    if !state.initialized {
        scan_controllers(&mut state);
        state.initialized = true;
        return;
    }

    // Periodic re-scan every ~5 seconds (150 ticks at 30Hz).
    // Picks up newly created controller tracks (e.g. after loading a demo).
    if state.tick_count % 150 == 0 {
        scan_controllers(&mut state);
    }

    let reaper = HighReaper::get();
    let medium = reaper.medium_reaper();

    // Use play position when playing/recording, edit cursor position when stopped.
    // This way scenes update when scrubbing/clicking the timeline, not just during playback.
    let play_state = medium.get_play_state_ex(ReaperProjectContext::CurrentProject);
    let position = if play_state.is_playing || play_state.is_recording {
        medium.get_play_position_ex(ReaperProjectContext::CurrentProject).get()
    } else {
        medium.low().GetCursorPosition()
    };

    // Check each controller for scene changes
    for ctrl in &mut state.controllers {
        // Find the timeline entry at current position
        let entry = ctrl
            .timeline
            .iter()
            .find(|e| position >= e.start && position < e.end);

        let target = entry.map(|e| e.index as i32).unwrap_or(-1);

        if target == ctrl.active_scene || target < 0 {
            continue;
        }

        let old = ctrl.active_scene;
        ctrl.active_scene = target;

        // Apply track muting — unmute the target child, mute all others
        if apply_track_switch(&ctrl.child_track_indices, target as u32) {
            let target_name = entry.map(|e| e.name.as_str()).unwrap_or("?");
            info!(
                "[scene-timer] '{}' → '{}' (scene {}, was {})",
                ctrl.name,
                target_name,
                target + 1,
                if old < 0 {
                    "none".to_string()
                } else {
                    (old + 1).to_string()
                }
            );
        } else {
            ctrl.active_scene = old; // revert on failure
        }
    }
}

/// Invalidate the cached controller state (e.g. after loading a demo).
pub fn invalidate() {
    if let Ok(mut state) = STATE.lock() {
        state.initialized = false;
        state.controllers.clear();
    }
}

/// Scan all tracks for Signal Controller instances and build the cache.
fn scan_controllers(state: &mut SceneState) {
    state.controllers.clear();

    let reaper = HighReaper::get();
    let project = reaper.current_project();
    let track_count = project.track_count();

    let low = reaper.medium_reaper().low();

    for track_idx in 0..track_count {
        let Some(track) = project.track_by_index(track_idx) else {
            continue;
        };

        // Check for fts_signal/scene_count in P_EXT
        let scene_count = match read_p_ext(low, &track, SCENE_COUNT_KEY) {
            Some(s) => match s.parse::<u32>() {
                Ok(n) if n > 0 => n,
                _ => continue,
            },
            None => continue,
        };

        // Check if this is a folder track
        let raw = match track.raw() {
            Ok(r) => r,
            Err(_) => continue,
        };
        let folder_depth = unsafe {
            low.GetMediaTrackInfo_Value(raw.as_ptr(), c"I_FOLDERDEPTH".as_ptr()) as i32
        };
        if folder_depth < 1 {
            continue; // Not a folder
        }

        let name = track.name().map(|n| n.to_string()).unwrap_or_default();

        // Find all non-folder child tracks (section/song tracks).
        // These are muted/unmuted directly for scene switching.
        let mut child_track_indices = Vec::new();
        for child_idx in (track_idx + 1)..track_count {
            let Some(child) = project.track_by_index(child_idx) else {
                break;
            };
            let child_raw = match child.raw() {
                Ok(r) => r,
                Err(_) => break,
            };

            // Check parent
            let parent_ptr =
                unsafe { low.GetParentTrack(child_raw.as_ptr()) };
            if parent_ptr != raw.as_ptr() {
                // Not our child — might have moved past our folder
                let child_depth = unsafe {
                    low.GetMediaTrackInfo_Value(child_raw.as_ptr(), c"I_FOLDERDEPTH".as_ptr())
                        as i32
                };
                if child_depth < 0 {
                    // This track closes a folder — continue scanning
                    continue;
                }
                break;
            }

            let child_folder = unsafe {
                low.GetMediaTrackInfo_Value(child_raw.as_ptr(), c"I_FOLDERDEPTH".as_ptr()) as i32
            };
            if child_folder < 1 {
                // Non-folder child = section/song track
                child_track_indices.push(child_idx);
            }
        }

        if child_track_indices.is_empty() {
            warn!("[scene-timer] No child tracks for controller '{name}'");
            continue;
        }

        // Read named items on the controller track to build timeline
        let timeline = read_item_timeline(low, raw.as_ptr());

        info!(
            "[scene-timer] Controller '{}': {} scenes, {} child tracks, {} items",
            name,
            scene_count,
            child_track_indices.len(),
            timeline.len(),
        );

        state.controllers.push(ControllerCache {
            child_track_indices,
            name,
            timeline,
            active_scene: -1, // unset — first poll will apply the correct scene
        });
    }

    info!(
        "[scene-timer] Scan complete: {} controller(s)",
        state.controllers.len()
    );
}

/// Read a P_EXT string value from a track.
fn read_p_ext(
    low: &LowReaper,
    track: &HighTrack,
    key: &str,
) -> Option<String> {
    let raw = track.raw().ok()?;
    let attr = CString::new(key).ok()?;
    let mut buf = vec![0u8; 256];
    let ok = unsafe {
        low.GetSetMediaTrackInfo_String(
            raw.as_ptr(),
            attr.as_ptr(),
            buf.as_mut_ptr() as *mut i8,
            false,
        )
    };
    if !ok {
        return None;
    }
    let nul = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    let s = std::str::from_utf8(&buf[..nul]).ok()?.to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Read items from a track, sorted by position, and assign sequential indices.
/// Each item's take name is captured for logging/identification.
fn read_item_timeline(
    low: &LowReaper,
    track_ptr: *mut MediaTrack,
) -> Vec<TimelineEntry> {
    let mut timeline = Vec::new();

    let item_count = unsafe { low.GetTrackNumMediaItems(track_ptr) };
    for i in 0..item_count {
        let item = unsafe { low.GetTrackMediaItem(track_ptr, i) };
        if item.is_null() {
            continue;
        }

        let start =
            unsafe { low.GetMediaItemInfo_Value(item, c"D_POSITION".as_ptr()) };
        let length =
            unsafe { low.GetMediaItemInfo_Value(item, c"D_LENGTH".as_ptr()) };
        let end = start + length;

        // Read take name
        let take = unsafe { low.GetActiveTake(item) };
        let name = if !take.is_null() {
            read_take_name(low, take)
        } else {
            String::new()
        };

        timeline.push(TimelineEntry {
            start,
            end,
            index: 0, // assigned after sorting
            name,
        });
    }

    // Sort by position, then assign sequential indices
    timeline.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap());
    for (i, entry) in timeline.iter_mut().enumerate() {
        entry.index = i as u8;
    }

    timeline
}

/// Read the name of a take.
fn read_take_name(
    low: &LowReaper,
    take: *mut MediaItem_Take,
) -> String {
    let ptr = unsafe { low.GetTakeName(take) };
    if ptr.is_null() {
        return String::new();
    }
    unsafe { std::ffi::CStr::from_ptr(ptr) }
        .to_str()
        .unwrap_or("")
        .to_string()
}

/// Mute all child tracks except the target. Returns true on success.
fn apply_track_switch(child_track_indices: &[u32], target_scene: u32) -> bool {
    let reaper = HighReaper::get();
    let project = reaper.current_project();
    let low = reaper.medium_reaper().low();

    for (i, &track_idx) in child_track_indices.iter().enumerate() {
        let Some(track) = project.track_by_index(track_idx) else {
            continue;
        };
        let track_raw = match track.raw() {
            Ok(r) => r,
            Err(_) => continue,
        };

        let mute = i as u32 != target_scene;
        let ok = unsafe {
            low.SetMediaTrackInfo_Value(
                track_raw.as_ptr(),
                c"B_MUTE".as_ptr(),
                if mute { 1.0 } else { 0.0 },
            )
        };
        if !ok {
            debug!(
                "[scene-timer] SetMediaTrackInfo_Value B_MUTE failed: track {} mute={}",
                track_idx, mute
            );
        }
    }

    true
}

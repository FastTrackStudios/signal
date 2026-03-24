//! Timer-based scene switching using the sync DAW API.
//!
//! Reads the playhead position and named MIDI items on controller tracks
//! to determine which scene should be active. Switches scenes by muting/
//! unmuting sends from the input track to section tracks.
//!
//! Scenes are matched by **name** — the MIDI item's take name must match
//! the destination track's name. This means:
//! - Adding a new track in a song folder automatically makes it available
//! - Place switch actions create items with the section name
//! - No manual index management needed

use std::collections::HashMap;
use std::sync::Mutex;
use tracing::{info, warn};

const SCENE_COUNT_SECTION: &str = "fts_signal";
const SCENE_COUNT_KEY: &str = "scene_count";
const INPUT_TRACK_KEY: &str = "input_track_guid";

struct TimelineEntry {
    start: f64,
    end: f64,
    /// Section name (from MIDI item take name) — matched to track names.
    name: String,
}

struct ControllerCache {
    /// GUID of the input track that has sends to section tracks.
    input_track_guid: String,
    /// Map of section track name → send index on the input track.
    /// Built by matching send destinations to child track names.
    name_to_send_index: HashMap<String, u32>,
    #[allow(dead_code)]
    controller_guid: String,
    name: String,
    timeline: Vec<TimelineEntry>,
    /// Currently active scene name (empty = none).
    active_scene: String,
}

struct SceneState {
    controllers: Vec<ControllerCache>,
    initialized: bool,
    tick_count: u32,
}

static STATE: Mutex<SceneState> = Mutex::new(SceneState {
    controllers: Vec::new(),
    initialized: false,
    tick_count: 0,
});

pub fn poll() {
    let Ok(mut state) = STATE.try_lock() else { return };

    state.tick_count += 1;

    if !state.initialized {
        scan_controllers(&mut state);
        state.initialized = true;
        return;
    }

    if state.tick_count % 150 == 0 {
        scan_controllers(&mut state);
    }

    let Some(daw) = daw::main_thread_daw() else { return };

    let Some(transport) = daw.transport_state() else { return };
    let is_playing = transport.play_state == daw::service::PlayState::Playing
        || transport.play_state == daw::service::PlayState::Recording;
    let position = if is_playing {
        transport.playhead_position.time.as_ref().map(|t| t.as_seconds())
    } else {
        transport.edit_position.time.as_ref().map(|t| t.as_seconds())
    };
    let Some(position) = position else { return };

    for ctrl in &mut state.controllers {
        // Find the timeline entry at the current position
        let entry = ctrl.timeline.iter().find(|e| position >= e.start && position < e.end);
        let target_name = entry.map(|e| e.name.as_str()).unwrap_or("");

        // Skip if nothing changed
        if target_name == ctrl.active_scene {
            continue;
        }

        let old = ctrl.active_scene.clone();
        ctrl.active_scene = target_name.to_string();

        if target_name.is_empty() {
            // No active scene at this position — mute ALL sends for this controller
            for (_, &send_idx) in &ctrl.name_to_send_index {
                daw.set_send_muted(&ctrl.input_track_guid, send_idx, true);
            }
            if !old.is_empty() {
                info!("[scene-timer] '{}' → (none) — all sends muted (was '{}')", ctrl.name, old);
            }
        } else if let Some(&target_send_idx) = ctrl.name_to_send_index.get(target_name) {
            // Active scene — unmute the target send, mute all others
            for (_, &send_idx) in &ctrl.name_to_send_index {
                daw.set_send_muted(&ctrl.input_track_guid, send_idx, send_idx != target_send_idx);
            }
            info!(
                "[scene-timer] '{}' → '{}' (send {}, was '{}')",
                ctrl.name, target_name, target_send_idx, old,
            );
        } else {
            warn!(
                "[scene-timer] '{}': no send found for scene '{}'",
                ctrl.name, target_name,
            );
            ctrl.active_scene = old;
        }
    }
}

pub fn invalidate() {
    if let Ok(mut state) = STATE.lock() {
        state.initialized = false;
        state.controllers.clear();
    }
}

fn scan_controllers(state: &mut SceneState) {
    state.controllers.clear();

    let Some(daw) = daw::main_thread_daw() else { return };
    let all_tracks = daw.track_list();

    for track_info in &all_tracks {
        if !track_info.is_folder { continue; }

        // Check for scene_count
        match daw.track_get_ext_state(&track_info.guid, SCENE_COUNT_SECTION, SCENE_COUNT_KEY) {
            Some(s) if s.parse::<u32>().is_ok() => {}
            _ => continue,
        };

        // Find the input track:
        // 1. Check this folder's P_EXT for input_track_guid
        // 2. Check parent folder's P_EXT (song folders inherit from rig folder)
        // 3. Fallback: first non-folder child of parent
        let input_track_guid = daw
            .track_get_ext_state(&track_info.guid, SCENE_COUNT_SECTION, INPUT_TRACK_KEY)
            .filter(|s| !s.is_empty())
            .or_else(|| {
                // Check parent folder's P_EXT
                let parent_guid = track_info.parent_guid.as_deref()?;
                daw.track_get_ext_state(parent_guid, SCENE_COUNT_SECTION, INPUT_TRACK_KEY)
                    .filter(|s| !s.is_empty())
            })
            .unwrap_or_else(|| {
                // Fallback: first non-folder child of parent (or self)
                let search_parent = track_info.parent_guid.as_deref().unwrap_or(&track_info.guid);
                all_tracks.iter()
                    .find(|t| t.parent_guid.as_deref() == Some(search_parent) && !t.is_folder)
                    .map(|t| t.guid.clone())
                    .unwrap_or_default()
            });

        if input_track_guid.is_empty() { continue; }

        // Collect this controller's child track GUIDs (sections that belong to this song/rig)
        let child_guids: Vec<&str> = all_tracks.iter()
            .filter(|t| t.parent_guid.as_deref() == Some(&*track_info.guid) && !t.is_folder)
            .map(|t| t.guid.as_str())
            .collect();

        // Build name → send index mapping by checking each send's destination.
        // Only include sends whose destination is a child of THIS controller folder.
        let mut name_to_send_index = HashMap::new();
        let send_count = daw.send_count(&input_track_guid);

        for send_idx in 0..send_count {
            if let Some(dest_guid) = daw.send_dest_guid(&input_track_guid, send_idx) {
                // Only include if destination is a child of this controller
                if child_guids.contains(&dest_guid.as_str()) {
                    if let Some(dest_track) = all_tracks.iter().find(|t| t.guid == dest_guid) {
                        name_to_send_index.insert(dest_track.name.clone(), send_idx);
                    }
                }
            }
        }

        if name_to_send_index.is_empty() {
            warn!("[scene-timer] No named sends for '{}'", track_info.name);
            continue;
        }

        let timeline = read_item_timeline(&daw, &track_info.guid);

        info!(
            "[scene-timer] Controller '{}': input={}, {} sends ({}), {} items",
            track_info.name,
            input_track_guid,
            name_to_send_index.len(),
            name_to_send_index.keys().cloned().collect::<Vec<_>>().join(", "),
            timeline.len(),
        );

        state.controllers.push(ControllerCache {
            input_track_guid,
            name_to_send_index,
            controller_guid: track_info.guid.clone(),
            name: track_info.name.clone(),
            timeline,
            active_scene: "__uninitialized__".to_string(), // Forces first-tick apply
        });
    }

    info!("[scene-timer] Scan complete: {} controller(s)", state.controllers.len());
}

fn read_item_timeline(daw: &daw::reaper::DawMainThread, track_guid: &str) -> Vec<TimelineEntry> {
    let items = daw.items(track_guid);
    let mut timeline: Vec<TimelineEntry> = items
        .into_iter()
        .map(|item| TimelineEntry {
            start: item.position,
            end: item.position + item.length,
            name: item.take_name,
        })
        .collect();

    timeline.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap());
    timeline
}

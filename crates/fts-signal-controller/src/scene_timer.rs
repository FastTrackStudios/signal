//! Timer-based scene switching using the sync DAW API.
//!
//! Runs at ~30Hz via `daw::register_timer`. Reads the playhead position
//! and named items on controller tracks to determine which scene should
//! be active, then mutes/unmutes sends from the input track to section
//! tracks (not the tracks themselves).

use std::sync::Mutex;
use tracing::{info, warn};

const SCENE_COUNT_SECTION: &str = "fts_signal";
const SCENE_COUNT_KEY: &str = "scene_count";
const INPUT_TRACK_KEY: &str = "input_track_guid";

struct TimelineEntry {
    start: f64,
    end: f64,
    index: u8,
    name: String,
}

struct ControllerCache {
    /// GUID of the input track that has sends to section tracks.
    input_track_guid: String,
    /// Number of sends on the input track (one per section).
    send_count: u32,
    #[allow(dead_code)]
    controller_guid: String,
    name: String,
    timeline: Vec<TimelineEntry>,
    active_scene: i32,
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
        let entry = ctrl.timeline.iter().find(|e| position >= e.start && position < e.end);
        let target = entry.map(|e| e.index as i32).unwrap_or(-1);

        if target == ctrl.active_scene || target < 0 { continue; }

        let old = ctrl.active_scene;
        ctrl.active_scene = target;

        // Mute/unmute sends from the input track
        apply_send_switch(&daw, &ctrl.input_track_guid, ctrl.send_count, target as u32);

        let target_name = entry.map(|e| e.name.as_str()).unwrap_or("?");
        info!(
            "[scene-timer] '{}' → '{}' (scene {}, was {})",
            ctrl.name, target_name, target + 1,
            if old < 0 { "none".to_string() } else { (old + 1).to_string() }
        );
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
        let count_str = match daw.track_get_ext_state(&track_info.guid, SCENE_COUNT_SECTION, SCENE_COUNT_KEY) {
            Some(s) if s.parse::<u32>().is_ok() => s,
            _ => continue,
        };
        let _ = count_str;

        // Find the input track GUID (stored by demo_setlist)
        let input_track_guid = match daw.track_get_ext_state(&track_info.guid, SCENE_COUNT_SECTION, INPUT_TRACK_KEY) {
            Some(guid) if !guid.is_empty() => guid,
            _ => {
                // Fallback: find the first non-folder child (Guitar Input)
                match all_tracks.iter().find(|t| {
                    t.parent_guid.as_deref() == Some(&*track_info.guid) && !t.is_folder
                }) {
                    Some(input) => input.guid.clone(),
                    None => continue,
                }
            }
        };

        // Count sends on the input track
        let send_count = daw.send_count(&input_track_guid);
        if send_count == 0 {
            // No sends — fall back to counting non-folder children
            let child_count = all_tracks.iter()
                .filter(|t| t.parent_guid.as_deref() == Some(&*track_info.guid) && !t.is_folder)
                .count();
            if child_count == 0 {
                warn!("[scene-timer] No sends or children for '{}'", track_info.name);
                continue;
            }
        }

        let timeline = read_item_timeline(&daw, &track_info.guid);

        info!(
            "[scene-timer] Controller '{}': input={}, {} sends, {} items",
            track_info.name, input_track_guid, send_count, timeline.len()
        );

        state.controllers.push(ControllerCache {
            input_track_guid,
            send_count,
            controller_guid: track_info.guid.clone(),
            name: track_info.name.clone(),
            timeline,
            active_scene: -1,
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
            index: 0,
            name: item.take_name,
        })
        .collect();

    timeline.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap());
    for (i, entry) in timeline.iter_mut().enumerate() {
        entry.index = i as u8;
    }
    timeline
}

/// Mute all sends except the target scene's send.
fn apply_send_switch(daw: &daw::reaper::DawMainThread, input_track_guid: &str, send_count: u32, target_scene: u32) {
    for i in 0..send_count {
        let should_mute = i != target_scene;
        daw.set_send_muted(input_track_guid, i, should_mute);
    }
}

//! Timer-based scene switching using the sync DAW API.
//!
//! Runs at ~30Hz via `daw::register_timer`. Uses `daw::main_thread_daw()`
//! for zero-overhead direct REAPER calls — no async, no RPC, no channels.

use std::sync::Mutex;
use tracing::{info, warn};

const SCENE_COUNT_SECTION: &str = "fts_signal";
const SCENE_COUNT_KEY: &str = "scene_count";

struct TimelineEntry {
    start: f64,
    end: f64,
    index: u8,
    name: String,
}

struct ControllerCache {
    child_guids: Vec<String>,
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

        if apply_track_switch(&daw, &ctrl.child_guids, target as u32) {
            let target_name = entry.map(|e| e.name.as_str()).unwrap_or("?");
            info!(
                "[scene-timer] '{}' → '{}' (scene {}, was {})",
                ctrl.name, target_name, target + 1,
                if old < 0 { "none".to_string() } else { (old + 1).to_string() }
            );
        } else {
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

        let count_str = match daw.track_get_ext_state(&track_info.guid, SCENE_COUNT_SECTION, SCENE_COUNT_KEY) {
            Some(s) if s.parse::<u32>().is_ok() => s,
            _ => continue,
        };
        let _ = count_str;

        let mut child_guids = Vec::new();
        for child in &all_tracks {
            if child.parent_guid.as_deref() == Some(&*track_info.guid) && !child.is_folder {
                child_guids.push(child.guid.clone());
            }
        }

        if child_guids.is_empty() {
            warn!("[scene-timer] No child tracks for '{}'", track_info.name);
            continue;
        }

        let timeline = read_item_timeline(&daw, &track_info.guid);

        info!(
            "[scene-timer] Controller '{}': {} children, {} items",
            track_info.name, child_guids.len(), timeline.len()
        );

        state.controllers.push(ControllerCache {
            child_guids,
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

fn apply_track_switch(daw: &daw::reaper::DawMainThread, child_guids: &[String], target_scene: u32) -> bool {
    for (i, guid) in child_guids.iter().enumerate() {
        let should_mute = i as u32 != target_scene;
        daw.track_set_muted(guid, should_mute);
    }
    true
}

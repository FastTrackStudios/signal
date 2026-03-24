//! Timer-based scene switching using the DAW API.
//!
//! Runs at ~30Hz via `daw::register_timer`. Reads the playhead position
//! and named items on controller tracks to determine which scene should
//! be active, then mutes/unmutes child tracks.
//!
//! Uses only `daw::get()` / `daw::block_on()` — no direct reaper-rs.

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

    let Some(daw) = daw::get() else { return };
    let position = daw::block_on(async {
        let project = daw.current_project().await.ok()?;
        let state = project.transport().get_state().await.ok()?;
        let is_playing = state.play_state == daw::service::PlayState::Playing
            || state.play_state == daw::service::PlayState::Recording;
        if is_playing {
            state.playhead_position.time.as_ref().map(|t| t.as_seconds())
        } else {
            state.edit_position.time.as_ref().map(|t| t.as_seconds())
        }
    });
    let Some(Some(position)) = position else { return };

    for ctrl in &mut state.controllers {
        let entry = ctrl.timeline.iter().find(|e| position >= e.start && position < e.end);
        let target = entry.map(|e| e.index as i32).unwrap_or(-1);

        if target == ctrl.active_scene || target < 0 { continue; }

        let old = ctrl.active_scene;
        ctrl.active_scene = target;

        if apply_track_switch(&ctrl.child_guids, target as u32) {
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

    let Some(daw) = daw::get() else { return };
    let result = daw::block_on(async {
        let project = daw.current_project().await.ok()?;
        let all_tracks = project.tracks().all().await.ok()?;
        let mut controllers = Vec::new();

        for track_info in &all_tracks {
            if !track_info.is_folder { continue; }

            let track = project.tracks().by_guid(&track_info.guid).await.ok()??;
            let count_str = track.get_ext_state(SCENE_COUNT_SECTION, SCENE_COUNT_KEY).await.ok()??;
            if count_str.parse::<u32>().is_err() { continue; }

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

            let timeline = read_item_timeline(&track).await;

            info!(
                "[scene-timer] Controller '{}': {} children, {} items",
                track_info.name, child_guids.len(), timeline.len()
            );

            controllers.push(ControllerCache {
                child_guids,
                controller_guid: track_info.guid.clone(),
                name: track_info.name.clone(),
                timeline,
                active_scene: -1,
            });
        }
        Some(controllers)
    });

    if let Some(Some(controllers)) = result {
        state.controllers = controllers;
    }
    info!("[scene-timer] Scan complete: {} controller(s)", state.controllers.len());
}

async fn read_item_timeline(track: &daw::TrackHandle) -> Vec<TimelineEntry> {
    let mut timeline = Vec::new();
    let items = track.items();
    let count = items.count().await.unwrap_or(0);

    for i in 0..count {
        let Some(item) = items.by_index(i).await.ok().flatten() else { continue };
        let pos = item.position().await.ok().map(|p| p.as_seconds()).unwrap_or(0.0);
        let length = item.length().await.ok().map(|d| d.as_seconds()).unwrap_or(0.0);
        let name = item.active_take().name().await.unwrap_or_default();

        timeline.push(TimelineEntry { start: pos, end: pos + length, index: 0, name });
    }

    timeline.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap());
    for (i, entry) in timeline.iter_mut().enumerate() {
        entry.index = i as u8;
    }
    timeline
}

fn apply_track_switch(child_guids: &[String], target_scene: u32) -> bool {
    let Some(daw) = daw::get() else { return false };
    let result = daw::block_on(async {
        let project = daw.current_project().await.ok()?;
        for (i, guid) in child_guids.iter().enumerate() {
            let Some(track) = project.tracks().by_guid(guid).await.ok()? else { continue };
            if i as u32 != target_scene {
                track.mute().await.ok()?;
            } else {
                track.unmute().await.ok()?;
            }
        }
        Some(true)
    });
    result.flatten().unwrap_or(false)
}

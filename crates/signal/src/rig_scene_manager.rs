//! Manages preloaded rig scene hierarchies for gapless scene switching.
//!
//! Creates an "Input: {rig_name}" track that sends to complete `[R]/[E]/[L]`
//! track hierarchies — one per scene variant. Switching between scenes
//! mutes/unmutes sends for <5ms gapless transitions with reverb tail ring-out.
//!
//! ```text
//! Input: Guitar Rig                        ← record-armed, sends to all scene folders
//! [R] Guitar Rig :: Clean                  ← active (folder unmuted, send unmuted)
//!   [E] Main Engine
//!     [L] Drive / [L] Amp / [L] Time
//! [R] Guitar Rig :: Lead                   ← preloaded (folder muted, send muted, 0 CPU)
//!   [E] Main Engine
//!     [L] Drive / [L] Amp / [L] Time
//! ```

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use moire::sync::{Mutex, RwLock};

use daw::{Project, TrackHandle};
use signal_live::engine::rig_scene_applier::{RigSceneApplier, RigSceneApplyError};
use signal_live::SignalLive;
use signal_proto::rig::{Rig, RigSceneId};

/// How long to wait before muting a demoted scene's rig folder, allowing
/// reverb/delay tails to ring out naturally.
const TAIL_MUTE_DELAY: std::time::Duration = std::time::Duration::from_secs(7);

/// State for a single preloaded rig scene track hierarchy.
struct SceneSlot {
    /// The `[R]` folder track for this scene.
    rig_track: TrackHandle,
    scene_name: String,
    scene_id: String,
    /// True if this scene was loaded from the preload phase.
    from_preload: bool,
}

/// Internal state managed by the `RigSceneManager`.
struct RigSceneState {
    /// The "Input: {rig_name}" track that holds sends to scene rig folders.
    input_track: TrackHandle,
    rig_name: String,
    project: Project,
    /// Currently active scene (unmuted send + unmuted folder).
    current: Option<SceneSlot>,
    /// Previous scene with tail still ringing out (muted send, unmuted folder for ~7s).
    tail: Option<SceneSlot>,
    /// Preloaded scene slots keyed by scene_id, ready for instant switching.
    preloaded: HashMap<String, SceneSlot>,
    /// GUIDs of rig tracks with pending delayed mutes.
    pending_mutes: Arc<Mutex<HashSet<String>>>,
    /// True once preloading has started.
    preloading_active: bool,
}

/// Manages preloaded rig scene hierarchies for gapless switching.
///
/// Mirrors [`ReaperPatchApplier`](crate::reaper_applier::ReaperPatchApplier)
/// but operates on complete rig folder hierarchies instead of single-FX child tracks.
pub struct RigSceneManager {
    state: RwLock<Option<RigSceneState>>,
    signal_live: Arc<SignalLive>,
}

impl RigSceneManager {
    pub fn new(signal_live: Arc<SignalLive>) -> Self {
        Self {
            state: RwLock::new("signal.rig_scene.state", None),
            signal_live,
        }
    }

    /// Set up the input track for rig scene switching.
    ///
    /// Creates (or finds) an "Input: {rig_name}" track with parent_send disabled,
    /// record-armed, and input monitoring enabled. Recovers existing scene tracks
    /// from a previous session by scanning `[R] {name} :: *` tracks.
    pub async fn set_target(
        &self,
        rig_name: impl Into<String>,
        project: Project,
    ) -> Result<(), RigSceneApplyError> {
        let rig_name = rig_name.into();
        let input_track_name = format!("Input: {rig_name}");
        let tracks = project.tracks();

        // Find or create the input track
        let input_track = match tracks.by_name(&input_track_name).await {
            Ok(Some(t)) => t,
            _ => {
                let t = tracks.add(&input_track_name, None).await.map_err(|e| {
                    RigSceneApplyError::DawError(format!("create input track: {e}"))
                })?;
                t.set_parent_send(false).await.map_err(|e| {
                    RigSceneApplyError::DawError(format!("disable parent send: {e}"))
                })?;
                t
            }
        };

        // Record-arm + input monitoring
        let _ = input_track.arm().await;
        let _ = input_track
            .set_input_monitoring(daw::service::InputMonitoringMode::Normal)
            .await;

        // Recover existing scene tracks from a previous session.
        let scene_prefix = format!("[R] {rig_name} :: ");
        let input_guid = input_track.guid().to_string();
        let mut recovered_preloaded = HashMap::new();
        let mut recovered_current: Option<SceneSlot> = None;

        if let Ok(all_tracks) = tracks.all().await {
            let input_sends = input_track.sends().all().await.unwrap_or_default();

            for track_info in &all_tracks {
                if !track_info.name.starts_with(&scene_prefix) {
                    continue;
                }
                if track_info.guid == input_guid {
                    continue;
                }

                let scene_name = track_info.name[scene_prefix.len()..].to_string();

                // Check send mute status
                let send_muted = input_sends
                    .iter()
                    .find(|s| s.dest_track_guid.as_deref() == Some(&track_info.guid))
                    .map(|s| s.muted)
                    .unwrap_or(true);

                let handle = match tracks.by_guid(&track_info.guid).await {
                    Ok(Some(h)) => h,
                    _ => continue,
                };

                if send_muted {
                    eprintln!(
                        "[INFO] Recovered preloaded rig scene '{}' from existing track",
                        scene_name
                    );
                    recovered_preloaded.insert(
                        scene_name.clone(),
                        SceneSlot {
                            rig_track: handle,
                            scene_name: scene_name.clone(),
                            scene_id: scene_name, // best-effort, real ID unavailable
                            from_preload: true,
                        },
                    );
                } else {
                    eprintln!(
                        "[INFO] Recovered active rig scene '{}' from existing track",
                        scene_name
                    );
                    recovered_current = Some(SceneSlot {
                        rig_track: handle,
                        scene_name: scene_name.clone(),
                        scene_id: scene_name,
                        from_preload: true,
                    });
                }
            }
        }

        let recovered_count =
            recovered_preloaded.len() + if recovered_current.is_some() { 1 } else { 0 };
        if recovered_count > 0 {
            eprintln!(
                "[INFO] Recovered {recovered_count} existing rig scene track(s) from REAPER"
            );
        }

        *self.state.write().await = Some(RigSceneState {
            input_track,
            rig_name,
            project,
            current: recovered_current,
            tail: None,
            preloaded: recovered_preloaded,
            pending_mutes: Arc::new(Mutex::new("signal.rig_scene.pending_mutes", HashSet::new())),
            preloading_active: false,
        });

        Ok(())
    }

    /// Clear the target (disconnects from DAW).
    pub async fn clear_target(&self) {
        *self.state.write().await = None;
    }

    /// Preload all scene variants of a rig as complete REAPER track hierarchies.
    ///
    /// For each scene: calls `signal_live.load_rig_to_daw()`, renames the rig track
    /// to `[R] {name} :: {scene_name}`, creates a muted send from input, and mutes
    /// the folder. The first scene (or default) is left active as `current`.
    pub async fn preload_all_scenes(
        &self,
        rig: &Rig,
        default_scene_id: Option<&RigSceneId>,
    ) -> Result<(), RigSceneApplyError> {
        // Mark preloading as active
        {
            let mut guard = self.state.write().await;
            if let Some(state) = guard.as_mut() {
                state.preloading_active = true;
            }
        }

        let scenes: Vec<_> = rig
            .variants
            .iter()
            .map(|s| (s.id.clone(), s.name.clone()))
            .collect();

        let default_id = default_scene_id
            .cloned()
            .or_else(|| rig.default_variant().map(|s| s.id.clone()));

        for (scene_id, scene_name) in &scenes {
            let is_default = default_id.as_ref() == Some(scene_id);

            // Skip if already preloaded or active
            {
                let guard = self.state.read().await;
                if let Some(state) = guard.as_ref() {
                    let id_str = scene_id.to_string();
                    if state.preloaded.contains_key(&id_str) {
                        continue;
                    }
                    if let Some(ref current) = state.current {
                        if current.scene_id == id_str {
                            continue;
                        }
                    }
                }
            }

            // Load the rig hierarchy for this scene
            let rig_load_result = self
                .signal_live
                .load_rig_to_daw(rig, Some(scene_id), &{
                    let guard = self.state.read().await;
                    let state = guard.as_ref().ok_or_else(|| {
                        RigSceneApplyError::NoTarget("no target configured".into())
                    })?;
                    state.project.clone()
                })
                .await
                .map_err(|e| RigSceneApplyError::LoadError(e))?;

            let rig_track = rig_load_result.rig_instance.rig_track;

            // Rename the rig track to include the scene name
            let display_name = {
                let guard = self.state.read().await;
                let state = guard.as_ref().ok_or_else(|| {
                    RigSceneApplyError::NoTarget("no target configured".into())
                })?;
                format!("[R] {} :: {}", state.rig_name, scene_name)
            };
            let _ = rig_track.rename(&display_name).await;

            // Create send from input → rig track, then set up mute state
            let mut guard = self.state.write().await;
            let state = guard.as_mut().ok_or_else(|| {
                RigSceneApplyError::NoTarget("no target configured".into())
            })?;

            state
                .input_track
                .sends()
                .add_to(rig_track.guid())
                .await
                .map_err(|e| {
                    RigSceneApplyError::DawError(format!("create send to scene: {e}"))
                })?;

            let scene_id_str = scene_id.to_string();

            if is_default {
                // Default scene: leave unmuted as current
                state.current = Some(SceneSlot {
                    rig_track,
                    scene_name: scene_name.clone(),
                    scene_id: scene_id_str,
                    from_preload: true,
                });
                eprintln!("[INFO] Preloaded default rig scene '{scene_name}' (active)");
            } else {
                // Non-default: mute send + mute folder to save CPU
                mute_send_to_track(&state.input_track, rig_track.guid()).await;
                let _ = rig_track.mute().await;

                state.preloaded.insert(
                    scene_id_str.clone(),
                    SceneSlot {
                        rig_track,
                        scene_name: scene_name.clone(),
                        scene_id: scene_id_str,
                        from_preload: true,
                    },
                );
                eprintln!("[INFO] Preloaded rig scene '{scene_name}' (muted)");
            }
        }

        Ok(())
    }

    /// Switch to a preloaded scene by scene_id.
    ///
    /// Fast path: mute current send, unmute preloaded, schedule tail mute.
    /// Returns `Ok(true)` on success, `Ok(false)` if scene not yet preloaded.
    async fn switch_scene_inner(
        &self,
        _rig_id: &str,
        scene_id: &str,
        scene_name: Option<&str>,
    ) -> Result<bool, RigSceneApplyError> {
        let mut guard = self.state.write().await;
        let state = guard
            .as_mut()
            .ok_or_else(|| RigSceneApplyError::NoTarget("no rig scene target configured".into()))?;

        let label = scene_name.unwrap_or(scene_id);

        // =================================================================
        // FAST PATH: switch to a preloaded scene (mute/unmute only, <5ms)
        // =================================================================
        if let Some(preloaded) = state.preloaded.remove(scene_id) {
            // Demote current scene
            if let Some(current) = state.current.take() {
                mute_send_to_track(&state.input_track, current.rig_track.guid()).await;

                if current.from_preload {
                    // Return to preload pool, schedule delayed folder mute
                    schedule_delayed_folder_mute(&current.rig_track, &state.pending_mutes).await;
                    state.preloaded.insert(
                        current.scene_id.clone(),
                        SceneSlot {
                            rig_track: current.rig_track,
                            scene_name: current.scene_name,
                            scene_id: current.scene_id,
                            from_preload: true,
                        },
                    );
                } else {
                    // Non-preloaded: becomes tail
                    state.tail = Some(current);
                }
            }

            // Cancel any pending delayed mute for this track
            cancel_pending_mute(preloaded.rig_track.guid(), &state.pending_mutes).await;

            // Unmute the folder first (resume FX processing), then unmute send
            let _ = preloaded.rig_track.unmute().await;
            unmute_send_to_track(&state.input_track, preloaded.rig_track.guid()).await;

            // Promote to current
            state.current = Some(SceneSlot {
                rig_track: preloaded.rig_track,
                scene_name: preloaded.scene_name,
                scene_id: preloaded.scene_id,
                from_preload: true,
            });

            eprintln!("[INFO] Fast-switched to preloaded rig scene '{label}'");
            return Ok(true);
        }

        // Also check by scene_name in case the caller used name-based lookup
        if let Some(scene_name_str) = scene_name {
            let found_key = state
                .preloaded
                .iter()
                .find(|(_, slot)| slot.scene_name == scene_name_str)
                .map(|(k, _)| k.clone());

            if let Some(key) = found_key {
                if let Some(preloaded) = state.preloaded.remove(&key) {
                    // Demote current scene
                    if let Some(current) = state.current.take() {
                        mute_send_to_track(&state.input_track, current.rig_track.guid()).await;
                        if current.from_preload {
                            schedule_delayed_folder_mute(&current.rig_track, &state.pending_mutes)
                                .await;
                            state.preloaded.insert(
                                current.scene_id.clone(),
                                SceneSlot {
                                    rig_track: current.rig_track,
                                    scene_name: current.scene_name,
                                    scene_id: current.scene_id,
                                    from_preload: true,
                                },
                            );
                        } else {
                            state.tail = Some(current);
                        }
                    }

                    cancel_pending_mute(preloaded.rig_track.guid(), &state.pending_mutes).await;
                    let _ = preloaded.rig_track.unmute().await;
                    unmute_send_to_track(&state.input_track, preloaded.rig_track.guid()).await;

                    state.current = Some(SceneSlot {
                        rig_track: preloaded.rig_track,
                        scene_name: preloaded.scene_name,
                        scene_id: preloaded.scene_id,
                        from_preload: true,
                    });

                    eprintln!("[INFO] Fast-switched to preloaded rig scene '{scene_name_str}' (by name)");
                    return Ok(true);
                }
            }
        }

        // =================================================================
        // GUARD: if preloading is active, don't fall through
        // =================================================================
        if state.preloading_active {
            eprintln!(
                "[INFO] Rig scene '{label}' not yet preloaded, skipping (preload in progress)"
            );
            return Ok(false);
        }

        eprintln!("[WARN] Rig scene '{label}' not found in preloaded scenes");
        Ok(false)
    }

    /// Returns the names of all currently preloaded scenes (for UI status display).
    pub async fn preloaded_scene_names(&self) -> Vec<String> {
        let guard = self.state.read().await;
        match guard.as_ref() {
            Some(state) => {
                let mut names: Vec<String> = state
                    .preloaded
                    .values()
                    .map(|s| s.scene_name.clone())
                    .collect();
                if let Some(ref current) = state.current {
                    if current.from_preload {
                        names.push(current.scene_name.clone());
                    }
                }
                if let Some(ref tail) = state.tail {
                    names.push(tail.scene_name.clone());
                }
                names
            }
            None => Vec::new(),
        }
    }
}

// ─── RigSceneApplier impl ──────────────────────────────────────

impl RigSceneApplier for RigSceneManager {
    fn switch_scene<'a>(
        &'a self,
        rig_id: &'a str,
        scene_id: &'a str,
        scene_name: Option<&'a str>,
    ) -> Pin<Box<dyn Future<Output = Result<bool, RigSceneApplyError>> + Send + 'a>> {
        Box::pin(self.switch_scene_inner(rig_id, scene_id, scene_name))
    }
}

// ─── Send helpers (mirrors reaper_applier.rs) ───────────────────

/// Find the send from `source_track` to `dest_guid` and mute it.
async fn mute_send_to_track(source_track: &TrackHandle, dest_guid: &str) -> bool {
    if let Ok(sends) = source_track.sends().all().await {
        for (i, send) in sends.iter().enumerate() {
            if send.dest_track_guid.as_deref() == Some(dest_guid) {
                if let Ok(Some(handle)) = source_track.sends().by_index(i as u32).await {
                    if handle.mute().await.is_ok() {
                        return true;
                    }
                }
                break;
            }
        }
    }
    false
}

/// Find the send from `source_track` to `dest_guid` and unmute it.
async fn unmute_send_to_track(source_track: &TrackHandle, dest_guid: &str) -> bool {
    if let Ok(sends) = source_track.sends().all().await {
        for (i, send) in sends.iter().enumerate() {
            if send.dest_track_guid.as_deref() == Some(dest_guid) {
                if let Ok(Some(handle)) = source_track.sends().by_index(i as u32).await {
                    if handle.unmute().await.is_ok() {
                        return true;
                    }
                }
                break;
            }
        }
    }
    false
}

/// Schedule a background task that mutes a rig folder after [`TAIL_MUTE_DELAY`].
///
/// The send is already muted (no new audio reaches the scene), but the folder
/// stays unmuted so reverb/delay tails ring out. After the delay, muting the
/// folder saves CPU.
async fn schedule_delayed_folder_mute(
    track: &TrackHandle,
    pending_mutes: &Arc<Mutex<HashSet<String>>>,
) {
    let guid = track.guid().to_string();
    let track = track.clone();
    let pending = Arc::clone(pending_mutes);

    pending.lock().await.insert(guid.clone());

    tokio::spawn(async move {
        tokio::time::sleep(TAIL_MUTE_DELAY).await;
        let should_mute = pending.lock().await.remove(&guid);
        if should_mute {
            let _ = track.mute().await;
            eprintln!("[INFO] Delayed folder mute applied to rig scene '{guid}'");
        } else {
            eprintln!("[INFO] Delayed folder mute cancelled for rig scene '{guid}' (re-activated)");
        }
    });
}

/// Cancel any pending delayed mute for the given track GUID.
async fn cancel_pending_mute(guid: &str, pending_mutes: &Arc<Mutex<HashSet<String>>>) {
    pending_mutes.lock().await.remove(guid);
}

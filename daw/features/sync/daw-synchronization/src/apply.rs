//! Applies remote sync events to the local DAW via daw-control mutations.
//!
//! Each [`SyncDomain`] variant is matched and translated into the appropriate
//! daw-control API call. The suppression set is updated before each mutation
//! so the next poll cycle knows to skip the resulting change.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::{SyncDomain, SyncEvent};
use daw::rpc::Daw;
use daw::service::{
    FxChainContext, FxEvent, ItemEvent, MarkerEvent, ProjectContext, RegionEvent,
    ReorderTracksBehavior, RoutingEvent, TakeEvent, TempoMapEvent, TrackEvent, TrackRef, Transport,
    routing::RouteType,
};
use tracing::{debug, info, warn};

use crate::drift::{DriftAction, DriftCorrector};
use crate::suppression::{SuppressionKey, SuppressionSet};

/// Maps remote GUIDs to local GUIDs for entities created by sync.
///
/// When a track is added on the master, the follower creates a new track with a
/// different GUID. This table lets subsequent events (rename, volume, etc.)
/// targeting the master's GUID resolve to the correct local track.
static GUID_MAP: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

fn map_guid(remote_guid: &str) -> Option<String> {
    GUID_MAP
        .lock()
        .ok()
        .and_then(|map| map.as_ref()?.get(remote_guid).cloned())
}

fn insert_guid_mapping(remote_guid: String, local_guid: String) {
    let mut lock = GUID_MAP.lock().unwrap();
    let map = lock.get_or_insert_with(HashMap::new);
    let normalized_remote = normalize_guid(&remote_guid).to_string();
    let normalized_local = normalize_guid(&local_guid).to_string();
    debug!(
        "GUID mapping: {} → {}",
        &normalized_remote[..8.min(normalized_remote.len())],
        &normalized_local[..8.min(normalized_local.len())]
    );
    map.insert(normalized_remote, normalized_local);
}

fn remove_guid_mapping(remote_guid: &str) {
    if let Ok(mut lock) = GUID_MAP.lock()
        && let Some(map) = lock.as_mut()
    {
        map.remove(normalize_guid(remote_guid));
    }
}

/// Strip braces from a GUID string (e.g., "{AAAA-BBBB}" → "AAAA-BBBB").
/// REAPER track GUIDs use the unbraced format, while item track_guid fields
/// use the braced format. Normalize to unbraced for consistent mapping lookups.
fn normalize_guid(guid: &str) -> &str {
    guid.strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .unwrap_or(guid)
}

/// Resolve a GUID: check the mapping table first, fall back to the original.
fn resolve_guid(remote_guid: &str) -> String {
    let normalized = normalize_guid(remote_guid);
    map_guid(normalized).unwrap_or_else(|| normalized.to_string())
}

/// Maps a (project_guid, remote_marker_id) → local_marker_id. Markers don't
/// expose GUIDs in our control API, so we keep an explicit id translation
/// for every marker created via sync.
static MARKER_ID_MAP: Mutex<Option<HashMap<(String, u32), u32>>> = Mutex::new(None);

fn insert_marker_mapping(project_guid: &str, remote_id: u32, local_id: u32) {
    let mut lock = MARKER_ID_MAP.lock().unwrap();
    let map = lock.get_or_insert_with(HashMap::new);
    map.insert((project_guid.to_string(), remote_id), local_id);
}

fn resolve_marker_id(project_guid: &str, remote_id: u32) -> u32 {
    MARKER_ID_MAP
        .lock()
        .ok()
        .and_then(|m| {
            m.as_ref()?
                .get(&(project_guid.to_string(), remote_id))
                .copied()
        })
        .unwrap_or(remote_id)
}

fn remove_marker_mapping(project_guid: &str, remote_id: u32) {
    if let Ok(mut lock) = MARKER_ID_MAP.lock()
        && let Some(map) = lock.as_mut()
    {
        map.remove(&(project_guid.to_string(), remote_id));
    }
}

/// Apply a remote sync domain event to the local DAW.
///
/// Wraps mutations in a REAPER undo block so Ctrl+Z undoes the entire sync
/// operation as a single step (not individual API calls).
///
/// The caller is responsible for inserting suppression keys into the suppression set.
#[allow(clippy::too_many_arguments)]
pub async fn apply_remote_event(
    daw: &Daw,
    project_guid: &str,
    domain: &SyncDomain,
    suppression: &mut SuppressionSet,
    drift_corrector: &mut DriftCorrector,
    link_active: bool,
    is_master: bool,
    event_created_at_ms: u64,
) {
    // Remote project GUIDs are process-local (e.g., REAPER pointer-based).
    // If the exact GUID isn't found locally, fall back to the current project.
    //
    // For transport events (high-frequency heartbeats), skip the project
    // resolution RPC — apply_transport resolves the project itself.
    let is_transport = matches!(domain, SyncDomain::Transport(_));
    let (resolved_guid, ctx) = if is_transport {
        (project_guid.to_string(), ProjectContext::Current)
    } else if daw.project(project_guid).await.is_ok() {
        (
            project_guid.to_string(),
            ProjectContext::Project(project_guid.to_string()),
        )
    } else {
        match daw.current_project().await {
            Ok(p) => match p.info().await {
                Ok(info) => (info.guid.clone(), ProjectContext::Project(info.guid)),
                Err(_) => (project_guid.to_string(), ProjectContext::Current),
            },
            Err(_) => (project_guid.to_string(), ProjectContext::Current),
        }
    };
    let project_guid = &resolved_guid;

    // Transport events (heartbeats) are high-frequency and don't modify the
    // project — skip undo blocks to avoid expensive SHM round-trips at 30Hz.
    let needs_undo = !matches!(domain, SyncDomain::Transport(_) | SyncDomain::Project(_));

    if needs_undo {
        let label = format!("FTS Sync: {:?}", domain_label(domain));
        if let Ok(project) = daw.project(project_guid).await {
            let _ = project.begin_undo_block(&label).await;
        }
    }

    match domain {
        SyncDomain::Transport(transport) => {
            apply_transport(
                daw,
                &ctx,
                project_guid,
                transport,
                suppression,
                drift_corrector,
                link_active,
                is_master,
                event_created_at_ms,
            )
            .await;
        }
        SyncDomain::Track(event) => {
            apply_track(daw, &ctx, event, suppression).await;
        }
        SyncDomain::Fx(event) => {
            apply_fx(daw, &ctx, event, suppression).await;
        }
        SyncDomain::Item(event) => {
            apply_item(daw, &ctx, event, suppression).await;
        }
        SyncDomain::Take(event) => {
            apply_take(daw, &ctx, event, suppression).await;
        }
        SyncDomain::Routing(event) => {
            apply_routing(daw, &ctx, event, suppression).await;
        }
        SyncDomain::TempoMap(event) => {
            apply_tempo_map(daw, &ctx, project_guid, event, suppression).await;
        }
        SyncDomain::Marker(event) => {
            apply_marker(daw, &ctx, project_guid, event, suppression).await;
        }
        SyncDomain::Region(event) => {
            apply_region(daw, &ctx, project_guid, event, suppression).await;
        }
        SyncDomain::Project(_event) => {
            // Project events (open/close) are informational — we don't
            // automatically open/close projects on remote peers.
            debug!("Received project event from remote peer (informational only)");
        }
    }

    if needs_undo {
        let label = format!("FTS Sync: {:?}", domain_label(domain));
        if let Ok(project) = daw.project(project_guid).await {
            let _ = project.end_undo_block(&label).await;
        }
    }
}

/// Short label for undo block display.
fn domain_label(domain: &SyncDomain) -> &'static str {
    match domain {
        SyncDomain::Transport(_) => "Transport",
        SyncDomain::Track(_) => "Track",
        SyncDomain::Fx(_) => "FX",
        SyncDomain::Item(_) => "Item",
        SyncDomain::Take(_) => "Take",
        SyncDomain::Routing(_) => "Routing",
        SyncDomain::TempoMap(_) => "Tempo",
        SyncDomain::Marker(_) => "Marker",
        SyncDomain::Region(_) => "Region",
        SyncDomain::Project(_) => "Project",
    }
}

// ── Transport ────────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
async fn apply_transport(
    daw: &Daw,
    _ctx: &ProjectContext,
    project_guid: &str,
    transport: &Transport,
    suppression: &mut SuppressionSet,
    drift_corrector: &mut DriftCorrector,
    link_active: bool,
    is_master: bool,
    event_created_at_ms: u64,
) {
    // Use current_project() directly — remote project GUIDs are process-local
    // and won't match. This avoids a failed RPC round-trip on every heartbeat.
    let project = match daw.current_project().await {
        Ok(p) => p,
        Err(e) => {
            warn!("Cannot apply transport: no current project: {e}");
            return;
        }
    };
    let t = project.transport();

    use daw::service::PlayState;

    // If both sides are stopped, sync edit cursor position changes without
    // triggering a full state transition (no suppression, no play state change).
    if transport.play_state == PlayState::Stopped
        && let Ok(local) = t.get_state().await
        && local.play_state == PlayState::Stopped
    {
        if let Some(ref edit_pos) = transport.edit_position.time {
            let local_edit = local
                .edit_position
                .time
                .as_ref()
                .map(|t| t.as_seconds())
                .unwrap_or(0.0);
            let remote_edit = edit_pos.as_seconds();
            if (remote_edit - local_edit).abs() > 0.001 {
                debug!("Syncing edit cursor: {local_edit:.3}s → {remote_edit:.3}s");
                let _ = t.set_position(remote_edit).await;
            }
        }
        // Also sync tempo if changed
        if (transport.tempo.bpm - local.tempo.bpm).abs() > 0.001 {
            let _ = t.set_tempo(transport.tempo.bpm).await;
        }
        return;
    }

    // Check if this is a drift-correction heartbeat (both sides playing).
    //
    // When Link is active, it handles beat-phase alignment (playrate nudging
    // for fractional-beat sync). PeerMesh still handles absolute timeline
    // position sync: if the positions diverge by more than the hard-seek
    // threshold (e.g., a late-joining follower), we hard-seek to catch up.
    //
    // The master never applies drift correction from follower heartbeats —
    // followers follow the master, not the other way around. Without this
    // guard, a follower broadcasting its stale position during stop→restart
    // causes the master to hard-seek backwards.
    if transport.play_state == PlayState::Playing
        && let Ok(local) = t.get_state().await
        && local.play_state == PlayState::Playing
    {
        if is_master {
            // Master is authoritative — skip follower drift correction
            return;
        }
        // Compensate for the time gap between reading the two positions:
        // - Master position was read at `event_created_at_ms`
        // - Local position was just read (via get_state above)
        // Both positions are stale, but the local position is more recent.
        // Adding the time gap to the master position aligns both to the
        // same effective timestamp.
        let local_read_at = SyncEvent::now_ms();
        if let (Some(remote_pos), Some(local_pos)) = (
            transport.playhead_position.time.as_ref(),
            local.playhead_position.time.as_ref(),
        ) {
            let latency_compensation = if event_created_at_ms > 0 {
                let elapsed_ms = local_read_at.saturating_sub(event_created_at_ms);
                (elapsed_ms as f64) / 1000.0
            } else {
                0.0
            };
            let estimated_remote = remote_pos.as_seconds() + latency_compensation;
            let drift = estimated_remote - local_pos.as_seconds();

            if link_active {
                // Link handles playrate nudging for beat-phase sync.
                // PeerMesh only does hard seeks for large position drifts
                // (e.g., late-joining follower, seek while playing).
                if drift.abs() > drift_corrector.hard_seek_threshold() {
                    info!(
                        "Hard seek (Link active): drift={drift:.3}s, seeking to {estimated_remote:.3}s"
                    );
                    let _ = t.set_position(estimated_remote).await;
                }
            } else {
                // No Link — full PeerMesh drift correction (playrate + hard seek)
                match drift_corrector.correct(drift, local.playrate) {
                    DriftAction::SetRate { new_playrate } => {
                        debug!("Drift correction: drift={drift:.4}s, playrate → {new_playrate:.4}");
                        let _ = t.set_playrate(new_playrate).await;
                    }
                    DriftAction::Reset => {
                        debug!("Drift within tolerance, resetting playrate");
                        let _ = t.set_playrate(1.0).await;
                    }
                    DriftAction::HardSeek => {
                        info!(
                            "Hard seek to estimated position {estimated_remote:.3}s (drift={drift:.3}s)"
                        );
                        let _ = t.set_position(estimated_remote).await;
                        let _ = t.set_playrate(1.0).await;
                    }
                    DriftAction::None => {}
                }
            }
        }
        // Also sync tempo if it drifted (only when Link isn't handling tempo)
        if !link_active && (transport.tempo.bpm - local.tempo.bpm).abs() > 0.001 {
            info!(
                "Syncing tempo: remote={:.1} local={:.1}",
                transport.tempo.bpm, local.tempo.bpm
            );
            let _ = t.set_tempo(transport.tempo.bpm).await;
        }
        return;
    }

    // Master is authoritative for transport state — never apply state
    // transitions (play/stop/pause) from follower peers. Without this guard,
    // echoed stop events from followers cause oscillation during restart.
    if is_master {
        debug!("Master ignoring transport state transition from follower");
        return;
    }

    // Full transport state transition (play/stop/pause) — suppress echo so
    // our subscription forwarder doesn't re-broadcast this change.
    suppression.suppress(SuppressionKey::transport(project_guid));

    // Reset drift corrector on transport state changes.
    drift_corrector.reset();

    // Apply position first, then play state, so the playhead is at the
    // right position when playback starts.
    let pos_secs = transport
        .playhead_position
        .time
        .as_ref()
        .map(|t| t.as_seconds());
    if let Some(pos) = pos_secs {
        info!(
            "Transport state transition: setting position to {pos:.3}s before {:?}",
            transport.play_state
        );
        if let Err(e) = t.set_position(pos).await {
            warn!("Failed to set transport position: {e}");
        }
    } else {
        info!(
            "Transport state transition: no position in event, applying {:?}",
            transport.play_state
        );
    }

    if let Err(e) = t.set_tempo(transport.tempo.bpm).await {
        warn!("Failed to set tempo: {e}");
    }

    if let Err(e) = t.set_loop(transport.looping).await {
        warn!("Failed to set loop state: {e}");
    }

    // Reset playrate on transport state transitions
    let _ = t.set_playrate(1.0).await;

    // Play state is applied last to avoid race conditions
    match transport.play_state {
        PlayState::Playing => {
            info!("Applying remote play command (pos={pos_secs:?})");
            match tokio::time::timeout(std::time::Duration::from_secs(5), t.play()).await {
                Ok(Ok(_)) => info!("Play command completed successfully"),
                Ok(Err(e)) => warn!("Play command returned error: {e}"),
                Err(_) => warn!("Play command TIMED OUT after 5s"),
            }
        }
        PlayState::Paused => {
            info!("Applying remote pause command");
            let _ = t.pause().await;
        }
        PlayState::Stopped => {
            info!("Applying remote stop command");
            let _ = t.stop().await;
        }
        _ => {}
    }
}

// ── Track ────────────────────────────────────────────────────────────────────

async fn apply_track(
    daw: &Daw,
    ctx: &ProjectContext,
    event: &TrackEvent,
    suppression: &mut SuppressionSet,
) {
    // Resolve remote GUID to local GUID for suppression keys.
    // The subscription emits events with the local GUID, so we must
    // suppress using the local GUID to prevent echo loops.
    let resolve = |remote_guid: &str| resolve_guid(remote_guid);

    match event {
        TrackEvent::VolumeChanged { guid, volume } => {
            // Value-aware suppression: only matches if the echoed event
            // carries the same volume bits we just applied. A genuine
            // local change to a different value still broadcasts.
            suppression.suppress_value(SuppressionKey::track(&resolve(guid), "volume"), *volume);
            apply_track_mutation(daw, ctx, guid, |handle| {
                let volume = *volume;
                Box::pin(async move { handle.set_volume(volume).await })
            })
            .await;
        }
        TrackEvent::PanChanged { guid, pan } => {
            suppression.suppress_value(SuppressionKey::track(&resolve(guid), "pan"), *pan);
            apply_track_mutation(daw, ctx, guid, |handle| {
                let pan = *pan;
                Box::pin(async move { handle.set_pan(pan).await })
            })
            .await;
        }
        TrackEvent::AutomationModeChanged { guid, mode } => {
            suppression.suppress(SuppressionKey::track(&resolve(guid), "automode"));
            let mode = *mode;
            apply_track_mutation(daw, ctx, guid, move |handle| {
                Box::pin(async move { handle.set_automation_mode(mode).await })
            })
            .await;
        }
        TrackEvent::InputMonitorChanged { guid, monitor } => {
            suppression.suppress(SuppressionKey::track(&resolve(guid), "recmon"));
            let monitor = *monitor;
            apply_track_mutation(daw, ctx, guid, move |handle| {
                Box::pin(async move { handle.set_input_monitor(monitor).await })
            })
            .await;
        }
        TrackEvent::PhaseInvertedChanged { guid, inverted } => {
            suppression.suppress(SuppressionKey::track(&resolve(guid), "phase"));
            let inverted = *inverted;
            apply_track_mutation(daw, ctx, guid, move |handle| {
                Box::pin(async move { handle.set_phase_inverted(inverted).await })
            })
            .await;
        }
        TrackEvent::MuteChanged { guid, muted } => {
            suppression.suppress(SuppressionKey::track(&resolve(guid), "muted"));
            let muted = *muted;
            apply_track_mutation(daw, ctx, guid, move |handle| {
                Box::pin(async move {
                    if muted {
                        handle.mute().await
                    } else {
                        handle.unmute().await
                    }
                })
            })
            .await;
        }
        TrackEvent::SoloChanged { guid, soloed } => {
            suppression.suppress(SuppressionKey::track(&resolve(guid), "soloed"));
            let soloed = *soloed;
            apply_track_mutation(daw, ctx, guid, move |handle| {
                Box::pin(async move {
                    if soloed {
                        handle.solo().await
                    } else {
                        handle.unsolo().await
                    }
                })
            })
            .await;
        }
        TrackEvent::ArmChanged { guid, armed } => {
            suppression.suppress(SuppressionKey::track(&resolve(guid), "armed"));
            let armed = *armed;
            apply_track_mutation(daw, ctx, guid, move |handle| {
                Box::pin(async move {
                    if armed {
                        handle.arm().await
                    } else {
                        handle.disarm().await
                    }
                })
            })
            .await;
        }
        TrackEvent::Renamed { guid, name } => {
            suppression.suppress(SuppressionKey::track(&resolve(guid), "name"));
            let name = name.clone();
            apply_track_mutation(daw, ctx, guid, move |handle| {
                Box::pin(async move { handle.rename(&name).await })
            })
            .await;
        }
        TrackEvent::ColorChanged { guid, color } => {
            suppression.suppress(SuppressionKey::track(&resolve(guid), "color"));
            let color = color.unwrap_or(0);
            apply_track_mutation(daw, ctx, guid, move |handle| {
                Box::pin(async move { handle.set_color(color).await })
            })
            .await;
        }
        TrackEvent::SelectionChanged { guid, selected } => {
            suppression.suppress(SuppressionKey::track(&resolve(guid), "selected"));
            let selected = *selected;
            apply_track_mutation(daw, ctx, guid, move |handle| {
                Box::pin(async move {
                    if selected {
                        handle.select().await
                    } else {
                        handle.deselect().await
                    }
                })
            })
            .await;
        }
        TrackEvent::TcpVisibilityChanged { guid, visible } => {
            suppression.suppress(SuppressionKey::track(&resolve(guid), "tcp_visible"));
            let visible = *visible;
            apply_track_mutation(daw, ctx, guid, move |handle| {
                Box::pin(async move {
                    let current = handle.info().await?;
                    handle
                        .set_visibility(visible, current.visible_in_mixer)
                        .await
                })
            })
            .await;
        }
        TrackEvent::MixerVisibilityChanged { guid, visible } => {
            suppression.suppress(SuppressionKey::track(&resolve(guid), "mixer_visible"));
            let visible = *visible;
            apply_track_mutation(daw, ctx, guid, move |handle| {
                Box::pin(async move {
                    let current = handle.info().await?;
                    handle.set_visibility(current.visible_in_tcp, visible).await
                })
            })
            .await;
        }
        TrackEvent::Added(track) => {
            let project = match resolve_project(daw, ctx).await {
                Some(p) => p,
                None => return,
            };
            let tracks = project.tracks();
            // Add at the same index if possible
            match tracks.add(&track.name, Some(track.index)).await {
                Ok(new_handle) => {
                    // Map the remote GUID to the local GUID so subsequent
                    // events (rename, volume, etc.) can find this track.
                    // Read the guid synchronously off the handle — avoids a
                    // round-trip and dodges the JIT/schema bug that hits
                    // Option<Track> responses through the vox local caller.
                    let local_guid = new_handle.guid().to_string();
                    suppression.suppress(SuppressionKey::track(&local_guid, "added"));
                    insert_guid_mapping(track.guid.clone(), local_guid);
                }
                Err(e) => {
                    warn!("Failed to add track '{}': {e}", track.name);
                }
            }
        }
        TrackEvent::Removed(guid) => {
            let local_guid = resolve_guid(guid);
            suppression.suppress(SuppressionKey::track(&local_guid, "removed"));
            let project = match resolve_project(daw, ctx).await {
                Some(p) => p,
                None => return,
            };
            let tracks = project.tracks();
            if let Err(e) = tracks.remove(TrackRef::Guid(local_guid)).await {
                warn!("Failed to remove track {}: {e}", &guid[..8.min(guid.len())]);
            }
            remove_guid_mapping(guid);
        }
        TrackEvent::Moved {
            guid, new_index, ..
        } => {
            suppression.suppress(SuppressionKey::track(&resolve(guid), "moved"));
            move_track_to_index(daw, ctx, guid, *new_index).await;
        }
        // Unlike the FX count below, this *is* a mutation — a track either
        // sends to its parent or does not, and the other DAW can be told.
        TrackEvent::ParentSendChanged { guid, enabled } => {
            suppression.suppress(SuppressionKey::track(&resolve(guid), "parentsend"));
            let enabled = *enabled;
            apply_track_mutation(daw, ctx, guid, move |handle| {
                Box::pin(async move { handle.set_parent_send(enabled).await })
            })
            .await;
        }
        // A report, not a mutation: a track's FX count is whatever its
        // chain contains, and there is nothing to set. The chain itself
        // syncs through `SyncDomain::Fx`, and applying this would at best
        // duplicate that and at worst fight it.
        TrackEvent::FxCountChanged { .. } => {}
        // Same shape: a count is whatever the routing contains, and
        // there is nothing to set. The routes themselves sync through
        // `SyncDomain::Routing`, and applying this would at best
        // duplicate that and at worst fight it.
        TrackEvent::RouteCountsChanged { .. } => {}
        // A peer's record input is about THEIR hardware. "Audio channel
        // 3" is a socket on one interface and something else entirely
        // on another, so copying it across would point a peer's track
        // at whatever happens to be plugged into that number. The patch
        // list is how inputs are shared between machines, and it
        // resolves roles through each room's own studio profile
        // precisely so this cannot happen.
        TrackEvent::RecordInputChanged { .. } => {}
        // Grouping is derived, not shared. Each peer's own grouping
        // watcher rebuilds FTS groups from the taxonomy every time a
        // track changes, deterministically, so a peer already has the
        // same groups without being told — and applying this would
        // fight that watcher over the same slots.
        TrackEvent::GroupingChanged { .. } => {}
        // How tall a panel is drawn is how the person at THIS desk
        // wants to look at the session. Two rooms working the same
        // material do not share a screen, and one engineer scrolling
        // to see the drums should not resize the other's arrange view.
        TrackEvent::HeightChanged { .. } => {}
        // Lanes hold takes, and takes are the one thing a second rig
        // records for itself. Applying a lane layout from a peer would
        // rename and re-mask lanes whose contents this machine has
        // never heard, which is how a comp points at the wrong take.
        TrackEvent::LanesChanged { .. } => {}
        // Folder depth IS shared — it is the shape of the session, and
        // two rooms disagreeing about which tracks are in the drum
        // folder is exactly the divergence sync exists to prevent.
        TrackEvent::FolderDepthChanged { guid, folder_depth } => {
            let local = resolve_guid(guid);
            suppression.suppress(SuppressionKey::track(&local, "folderdepth"));
            if let Some(project) = resolve_project(daw, ctx).await
                && let Ok(Some(track)) = project.tracks().by_guid(&local).await
                && let Err(error) = track.set_folder_depth(*folder_depth).await
            {
                tracing::warn!(error = %error, "peer folder depth refused");
            }
        }
    }
}

async fn move_track_to_index(daw: &Daw, ctx: &ProjectContext, remote_guid: &str, new_index: u32) {
    let local_guid = resolve_guid(remote_guid);
    let project = match resolve_project(daw, ctx).await {
        Some(p) => p,
        None => return,
    };
    let tracks = project.tracks();

    let selected_guids = match tracks.all().await {
        Ok(tracks) => tracks
            .into_iter()
            .filter(|track| track.selected)
            .map(|track| track.guid)
            .collect::<Vec<_>>(),
        Err(e) => {
            warn!("Failed to read track selection before move: {e}");
            Vec::new()
        }
    };

    let handle = match tracks.by_guid(&local_guid).await {
        Ok(Some(handle)) => handle,
        Ok(None) => {
            warn!(
                "Track not found for move: {}",
                &remote_guid[..8.min(remote_guid.len())]
            );
            return;
        }
        Err(e) => {
            warn!(
                "Failed to resolve track {} for move: {e}",
                &remote_guid[..8.min(remote_guid.len())]
            );
            return;
        }
    };

    if let Err(e) = tracks.clear_selection().await {
        warn!("Failed to clear selection before moving track: {e}");
        return;
    }
    if let Err(e) = handle.select().await {
        warn!("Failed to select track before move: {e}");
        return;
    }
    if let Err(e) = tracks
        .reorder_selected(new_index, ReorderTracksBehavior::Normal)
        .await
    {
        warn!("Failed to move track to index {new_index}: {e}");
    }

    if let Err(e) = tracks.clear_selection().await {
        warn!("Failed to clear temporary move selection: {e}");
        return;
    }
    for guid in selected_guids {
        match tracks.by_guid(&guid).await {
            Ok(Some(handle)) => {
                if let Err(e) = handle.select().await {
                    warn!(
                        "Failed to restore selected track {}: {e}",
                        &guid[..8.min(guid.len())]
                    );
                }
            }
            Ok(None) => {}
            Err(e) => warn!(
                "Failed to restore selected track {}: {e}",
                &guid[..8.min(guid.len())]
            ),
        }
    }
}

/// Helper to resolve a project and track, then apply a mutation.
///
/// Uses the GUID mapping table to translate remote GUIDs to local GUIDs,
/// since synced tracks are created with different GUIDs on each instance.
async fn apply_track_mutation<F>(daw: &Daw, ctx: &ProjectContext, remote_guid: &str, mutation: F)
where
    F: FnOnce(
        daw::rpc::TrackHandle,
    )
        -> std::pin::Pin<Box<dyn std::future::Future<Output = daw::rpc::Result<()>> + Send>>,
{
    let local_guid = resolve_guid(remote_guid);
    let project = match resolve_project(daw, ctx).await {
        Some(p) => p,
        None => return,
    };
    let handle = match project.tracks().by_guid(&local_guid).await {
        Ok(Some(h)) => h,
        Ok(None) => {
            debug!(
                "Track {} not found (remote: {}), skipping mutation",
                &local_guid[..8.min(local_guid.len())],
                &remote_guid[..8.min(remote_guid.len())]
            );
            return;
        }
        Err(e) => {
            warn!(
                "Failed to resolve track {}: {e}",
                &local_guid[..8.min(local_guid.len())]
            );
            return;
        }
    };
    if let Err(e) = mutation(handle).await {
        warn!(
            "Track mutation failed for {}: {e}",
            &local_guid[..8.min(local_guid.len())]
        );
    }
}

// ── FX ───────────────────────────────────────────────────────────────────────

async fn apply_fx(
    daw: &Daw,
    ctx: &ProjectContext,
    event: &FxEvent,
    suppression: &mut SuppressionSet,
) {
    match event {
        FxEvent::ParameterChanged {
            context,
            fx_guid,
            param_index,
            value,
        } => {
            let context_key = format!("{context:?}");
            suppression.suppress(SuppressionKey::fx_param(
                &context_key,
                fx_guid,
                *param_index,
            ));
            if let Some(chain) = resolve_fx_chain(daw, ctx, context).await {
                if let Ok(Some(fx)) = chain.by_guid(fx_guid).await {
                    if let Err(e) = fx.param(*param_index).set(*value).await {
                        warn!("Failed to set FX param {param_index} on {fx_guid}: {e}");
                    }
                } else {
                    debug!("FX {fx_guid} not found in chain");
                }
            }
        }
        FxEvent::Added { context, fx } => {
            let context_key = format!("{context:?}");
            suppression.suppress(SuppressionKey::fx_param(&context_key, &fx.guid, 0));
            if let Some(chain) = resolve_fx_chain(daw, ctx, context).await {
                match chain.add(&fx.name).await {
                    Ok(handle) => {
                        if !fx.enabled {
                            let _ = handle.disable().await;
                        }
                    }
                    Err(e) => warn!("Failed to add FX '{}': {e}", fx.name),
                }
            }
        }
        FxEvent::Removed { context, fx_guid } => {
            let context_key = format!("{context:?}");
            suppression.suppress(SuppressionKey::fx_param(&context_key, fx_guid, 0));
            if let Some(chain) = resolve_fx_chain(daw, ctx, context).await
                && let Ok(Some(fx)) = chain.by_guid(fx_guid).await
                && let Err(e) = fx.remove().await
            {
                warn!("Failed to remove FX {fx_guid}: {e}");
            }
        }
        FxEvent::EnabledChanged {
            context,
            fx_guid,
            enabled,
        } => {
            let context_key = format!("{context:?}");
            suppression.suppress(SuppressionKey::fx_param(&context_key, fx_guid, 0));
            if let Some(chain) = resolve_fx_chain(daw, ctx, context).await
                && let Ok(Some(fx)) = chain.by_guid(fx_guid).await
            {
                let result = if *enabled {
                    fx.enable().await
                } else {
                    fx.disable().await
                };
                if let Err(e) = result {
                    warn!("Failed to set FX enabled={enabled} on {fx_guid}: {e}");
                }
            }
        }
        FxEvent::Moved {
            context,
            fx_guid,
            new_index,
            ..
        } => {
            let context_key = format!("{context:?}");
            suppression.suppress(SuppressionKey::fx_param(&context_key, fx_guid, 0));
            if let Some(chain) = resolve_fx_chain(daw, ctx, context).await
                && let Ok(Some(fx)) = chain.by_guid(fx_guid).await
                && let Err(e) = fx.move_to(*new_index).await
            {
                warn!("Failed to move FX {fx_guid} to index {new_index}: {e}");
            }
        }
        FxEvent::PresetChanged {
            context,
            fx_guid,
            preset_name,
        } => {
            let context_key = format!("{context:?}");
            suppression.suppress(SuppressionKey::fx_param(&context_key, fx_guid, 0));
            debug!(
                "FX preset changed: {fx_guid} → {:?} (preset sync requires index)",
                preset_name
            );
        }
        FxEvent::WindowChanged {
            context,
            fx_guid,
            open,
        } => {
            let context_key = format!("{context:?}");
            suppression.suppress(SuppressionKey::fx_param(&context_key, fx_guid, 0));
            if let Some(chain) = resolve_fx_chain(daw, ctx, context).await
                && let Ok(Some(fx)) = chain.by_guid(fx_guid).await
            {
                let result = if *open {
                    fx.open_ui().await
                } else {
                    fx.close_ui().await
                };
                if let Err(e) = result {
                    warn!("Failed to set FX window open={open} on {fx_guid}: {e}");
                }
            }
        }
        FxEvent::ContainerCreated { context, name, .. } => {
            if let Some(chain) = resolve_fx_chain(daw, ctx, context).await {
                let count = chain.count().await.unwrap_or(0);
                if let Err(e) = chain.create_container(name, count).await {
                    warn!("Failed to create FX container '{name}': {e}");
                }
            }
        }
        FxEvent::ContainerRemoved {
            context,
            container_id,
        } => {
            if let Some(chain) = resolve_fx_chain(daw, ctx, context).await
                && let Err(e) = chain.explode_container(container_id).await
            {
                warn!("Failed to remove FX container: {e}");
            }
        }
        FxEvent::RoutingModeChanged {
            context,
            container_id,
            mode,
        } => {
            if let Some(chain) = resolve_fx_chain(daw, ctx, context).await
                && let Err(e) = chain.set_routing_mode(container_id, *mode).await
            {
                warn!("Failed to set container routing mode: {e}");
            }
        }
        FxEvent::MovedToContainer {
            context,
            node_id,
            dest_container,
            ..
        } => {
            if let Some(chain) = resolve_fx_chain(daw, ctx, context).await
                && let Err(e) = chain.move_to_container(node_id, dest_container, 0).await
            {
                warn!("Failed to move FX to container: {e}");
            }
        }
        FxEvent::ContainerRenamed {
            context,
            container_id,
            name,
        } => {
            if let Some(chain) = resolve_fx_chain(daw, ctx, context).await
                && let Err(e) = chain.rename_container(container_id, name).await
            {
                warn!("Failed to rename FX container: {e}");
            }
        }
        FxEvent::TreeStructureChanged { context } => {
            debug!(
                "FX tree structure changed for {context:?} (no automatic sync for bulk changes)"
            );
        }
    }
}

/// Resolve an FxChainContext to an FxChain handle.
async fn resolve_fx_chain(
    daw: &Daw,
    ctx: &ProjectContext,
    fx_ctx: &FxChainContext,
) -> Option<daw::rpc::FxChain> {
    let project = resolve_project(daw, ctx).await?;
    let remote_track_guid = match fx_ctx {
        FxChainContext::Track(guid) | FxChainContext::Input(guid) => guid,
        FxChainContext::Monitoring => {
            debug!("Monitoring FX chain sync not yet supported");
            return None;
        }
    };
    let local_track_guid = resolve_guid(remote_track_guid);
    match project.tracks().by_guid(&local_track_guid).await {
        Ok(Some(track)) => {
            let chain = match fx_ctx {
                FxChainContext::Track(_) => track.fx_chain(),
                FxChainContext::Input(_) => track.input_fx_chain(),
                _ => unreachable!(),
            };
            Some(chain)
        }
        Ok(None) => {
            debug!("Track {local_track_guid} not found for FX chain resolution");
            None
        }
        Err(e) => {
            warn!("Failed to resolve track {local_track_guid} for FX chain: {e}");
            None
        }
    }
}

// ── Item ─────────────────────────────────────────────────────────────────────

async fn apply_item(
    daw: &Daw,
    ctx: &ProjectContext,
    event: &ItemEvent,
    suppression: &mut SuppressionSet,
) {
    match event {
        ItemEvent::PositionChanged {
            item_guid,
            new_position,
            ..
        } => {
            suppression.suppress(SuppressionKey::item(item_guid, "position"));
            apply_item_mutation(daw, ctx, item_guid, |handle| {
                let pos = *new_position;
                Box::pin(async move {
                    handle
                        .set_position(daw::service::primitives::PositionInSeconds::from_seconds(
                            pos,
                        ))
                        .await
                })
            })
            .await;
        }
        ItemEvent::LengthChanged {
            item_guid,
            new_length,
            ..
        } => {
            suppression.suppress(SuppressionKey::item(item_guid, "length"));
            apply_item_mutation(daw, ctx, item_guid, |handle| {
                let len = *new_length;
                Box::pin(async move {
                    handle
                        .set_length(daw::service::primitives::Duration::from_seconds(len))
                        .await
                })
            })
            .await;
        }
        ItemEvent::MuteChanged {
            item_guid, muted, ..
        } => {
            suppression.suppress(SuppressionKey::item(item_guid, "muted"));
            let muted = *muted;
            apply_item_mutation(daw, ctx, item_guid, move |handle| {
                Box::pin(async move {
                    if muted {
                        handle.mute().await
                    } else {
                        handle.unmute().await
                    }
                })
            })
            .await;
        }
        ItemEvent::VolumeChanged {
            item_guid, volume, ..
        } => {
            suppression.suppress(SuppressionKey::item(item_guid, "volume"));
            let volume = *volume;
            apply_item_mutation(daw, ctx, item_guid, move |handle| {
                Box::pin(async move { handle.set_volume(volume).await })
            })
            .await;
        }
        ItemEvent::Created {
            track_guid, item, ..
        } => {
            suppression.suppress(SuppressionKey::item(&item.guid, "created"));
            let local_track_guid = resolve_guid(track_guid);
            if let Some(project) = resolve_project(daw, ctx).await {
                if let Ok(Some(track)) = project.tracks().by_guid(&local_track_guid).await {
                    let pos = daw::service::primitives::PositionInSeconds::from_seconds(
                        item.position.as_seconds(),
                    );
                    let len =
                        daw::service::primitives::Duration::from_seconds(item.length.as_seconds());
                    match track.items().add(pos, len).await {
                        Ok(new_item) => {
                            // Use the handle's local guid synchronously
                            // instead of calling info().await — saves an RPC
                            // round-trip in a hot path and avoids the
                            // Option<Item> response type that's expensive
                            // to JIT compile on first use.
                            let local_guid = new_item.guid().to_string();
                            suppression.suppress(SuppressionKey::item(&local_guid, "created"));
                            insert_guid_mapping(item.guid.clone(), local_guid);
                        }
                        Err(e) => {
                            warn!("Failed to add item on track {local_track_guid}: {e}");
                        }
                    }
                } else {
                    warn!(
                        "Track not found for item creation: remote={track_guid}, resolved={local_track_guid}"
                    );
                }
            }
        }
        ItemEvent::Deleted { item_guid, .. } => {
            suppression.suppress(SuppressionKey::item(item_guid, "deleted"));
            apply_item_mutation(daw, ctx, item_guid, |handle| {
                Box::pin(async move { handle.delete().await })
            })
            .await;
        }
        ItemEvent::MovedToTrack {
            item_guid,
            new_track_guid,
            ..
        } => {
            suppression.suppress(SuppressionKey::item(item_guid, "moved_to_track"));
            let local_track_guid = resolve_guid(new_track_guid);
            apply_item_mutation(daw, ctx, item_guid, |handle| {
                let track = TrackRef::Guid(local_track_guid);
                Box::pin(async move { handle.move_to_track(track).await })
            })
            .await;
        }
        ItemEvent::SelectionChanged {
            item_guid,
            selected,
            ..
        } => {
            suppression.suppress(SuppressionKey::item(item_guid, "selected"));
            let selected = *selected;
            apply_item_mutation(daw, ctx, item_guid, move |handle| {
                Box::pin(async move {
                    if selected {
                        handle.select().await
                    } else {
                        handle.deselect().await
                    }
                })
            })
            .await;
        }
        ItemEvent::ActiveTakeChanged {
            item_guid,
            new_take_index,
            ..
        } => {
            suppression.suppress(SuppressionKey::item(item_guid, "active_take"));
            let idx = *new_take_index;
            let local_item_guid = resolve_guid(item_guid);
            if let Some(project) = resolve_project(daw, ctx).await
                && let Ok(Some(item)) = project.items().by_guid(&local_item_guid).await
                && let Ok(Some(take)) = item.takes().by_index(idx).await
                && let Err(e) = take.make_active().await
            {
                warn!("Failed to set active take {idx} on item {item_guid}: {e}");
            }
        }
        // Names no property and carries no value, so there is nothing
        // here to replay; a peer that wants these re-reads the item.
        ItemEvent::Changed { .. } => {}
    }
}

/// Helper to resolve a project and item by GUID, then apply a mutation.
async fn apply_item_mutation<F>(daw: &Daw, ctx: &ProjectContext, item_guid: &str, mutation: F)
where
    F: FnOnce(
        daw::rpc::ItemHandle,
    )
        -> std::pin::Pin<Box<dyn std::future::Future<Output = daw::rpc::Result<()>> + Send>>,
{
    let local_guid = resolve_guid(item_guid);
    let project = match resolve_project(daw, ctx).await {
        Some(p) => p,
        None => return,
    };
    let handle = match project.items().by_guid(&local_guid).await {
        Ok(Some(h)) => h,
        Ok(None) => {
            debug!("Item {item_guid} not found, skipping mutation");
            return;
        }
        Err(e) => {
            warn!("Failed to resolve item {item_guid}: {e}");
            return;
        }
    };
    if let Err(e) = mutation(handle).await {
        warn!("Item mutation failed for {item_guid}: {e}");
    }
}

// ── Take ─────────────────────────────────────────────────────────────────────

async fn apply_take(
    daw: &Daw,
    ctx: &ProjectContext,
    event: &TakeEvent,
    suppression: &mut SuppressionSet,
) {
    match event {
        TakeEvent::NameChanged {
            item_guid,
            take_guid,
            name,
            ..
        } => {
            suppression.suppress(SuppressionKey::item(item_guid, "take_name"));
            apply_take_mutation(daw, ctx, item_guid, take_guid, |handle| {
                let name = name.clone();
                Box::pin(async move { handle.set_name(&name).await })
            })
            .await;
        }
        TakeEvent::PitchChanged {
            item_guid,
            take_guid,
            pitch,
            ..
        } => {
            suppression.suppress(SuppressionKey::item(item_guid, "take_pitch"));
            let pitch = *pitch;
            apply_take_mutation(daw, ctx, item_guid, take_guid, move |handle| {
                Box::pin(async move { handle.set_pitch(pitch).await })
            })
            .await;
        }
        TakeEvent::PlayRateChanged {
            item_guid,
            take_guid,
            play_rate,
            ..
        } => {
            suppression.suppress(SuppressionKey::item(item_guid, "take_play_rate"));
            let rate = *play_rate;
            apply_take_mutation(daw, ctx, item_guid, take_guid, move |handle| {
                Box::pin(async move { handle.set_play_rate(rate).await })
            })
            .await;
        }
        TakeEvent::VolumeChanged {
            item_guid,
            take_guid,
            volume,
            ..
        } => {
            suppression.suppress(SuppressionKey::item(item_guid, "take_volume"));
            let volume = *volume;
            apply_take_mutation(daw, ctx, item_guid, take_guid, move |handle| {
                Box::pin(async move { handle.set_volume(volume).await })
            })
            .await;
        }
        TakeEvent::SourceChanged {
            item_guid,
            take_guid,
            source_path,
            ..
        } => {
            suppression.suppress(SuppressionKey::item(item_guid, "take_source"));
            if let Some(path) = source_path {
                let path = path.clone();
                apply_take_mutation(daw, ctx, item_guid, take_guid, move |handle| {
                    Box::pin(async move { handle.set_source_file(&path).await })
                })
                .await;
            }
        }
        TakeEvent::Created { item_guid, .. } => {
            suppression.suppress(SuppressionKey::item(item_guid, "take_created"));
            if let Some(project) = resolve_project(daw, ctx).await
                && let Ok(Some(item)) = project.items().by_guid(item_guid).await
                && let Err(e) = item.takes().add().await
            {
                warn!("Failed to add take to item {item_guid}: {e}");
            }
        }
        TakeEvent::Deleted {
            item_guid,
            take_guid,
            ..
        } => {
            suppression.suppress(SuppressionKey::item(item_guid, "take_deleted"));
            apply_take_mutation(daw, ctx, item_guid, take_guid, |handle| {
                Box::pin(async move { handle.delete().await })
            })
            .await;
        }
        // Carries no value — nothing to replay (see `ItemEvent::Changed`).
        TakeEvent::Changed { .. } => {}
    }
}

/// Helper to resolve a project, item, and take by GUIDs, then apply a mutation.
async fn apply_take_mutation<F>(
    daw: &Daw,
    ctx: &ProjectContext,
    item_guid: &str,
    take_guid: &str,
    mutation: F,
) where
    F: FnOnce(
        daw::rpc::TakeHandle,
    )
        -> std::pin::Pin<Box<dyn std::future::Future<Output = daw::rpc::Result<()>> + Send>>,
{
    let project = match resolve_project(daw, ctx).await {
        Some(p) => p,
        None => return,
    };
    let item = match project.items().by_guid(item_guid).await {
        Ok(Some(i)) => i,
        Ok(None) => {
            debug!("Item {item_guid} not found for take mutation");
            return;
        }
        Err(e) => {
            warn!("Failed to resolve item {item_guid}: {e}");
            return;
        }
    };
    // Find the take by GUID — iterate all takes and match by index
    let takes = match item.takes().all().await {
        Ok(t) => t,
        Err(e) => {
            warn!("Failed to get takes for item {item_guid}: {e}");
            return;
        }
    };
    let take_index = takes
        .iter()
        .enumerate()
        .find(|(_, t)| t.guid == take_guid)
        .map(|(i, _)| i as u32);
    let Some(idx) = take_index else {
        debug!("Take {take_guid} not found in item {item_guid}");
        return;
    };
    let take_handle = match item.takes().by_index(idx).await {
        Ok(Some(h)) => h,
        _ => {
            debug!("Take at index {idx} not found for item {item_guid}");
            return;
        }
    };
    if let Err(e) = mutation(take_handle).await {
        warn!("Take mutation failed for {take_guid} on item {item_guid}: {e}");
    }
}

// ── Routing ──────────────────────────────────────────────────────────────────

async fn apply_routing(
    daw: &Daw,
    ctx: &ProjectContext,
    event: &RoutingEvent,
    suppression: &mut SuppressionSet,
) {
    match event {
        RoutingEvent::VolumeChanged {
            source_track_guid,
            route_type,
            route_index,
            volume,
            ..
        } => {
            suppression.suppress(SuppressionKey::routing(
                source_track_guid,
                &format!("volume:{route_index}"),
            ));
            apply_route_mutation(
                daw,
                ctx,
                source_track_guid,
                *route_type,
                *route_index,
                |handle| {
                    let volume = *volume;
                    Box::pin(async move { handle.set_volume(volume).await })
                },
            )
            .await;
        }
        RoutingEvent::PanChanged {
            source_track_guid,
            route_type,
            route_index,
            pan,
            ..
        } => {
            suppression.suppress(SuppressionKey::routing(
                source_track_guid,
                &format!("pan:{route_index}"),
            ));
            apply_route_mutation(
                daw,
                ctx,
                source_track_guid,
                *route_type,
                *route_index,
                |handle| {
                    let pan = *pan;
                    Box::pin(async move { handle.set_pan(pan).await })
                },
            )
            .await;
        }
        RoutingEvent::MuteChanged {
            source_track_guid,
            route_type,
            route_index,
            muted,
            ..
        } => {
            suppression.suppress(SuppressionKey::routing(
                source_track_guid,
                &format!("mute:{route_index}"),
            ));
            let muted = *muted;
            apply_route_mutation(
                daw,
                ctx,
                source_track_guid,
                *route_type,
                *route_index,
                move |handle| {
                    Box::pin(async move {
                        if muted {
                            handle.mute().await
                        } else {
                            handle.unmute().await
                        }
                    })
                },
            )
            .await;
        }
        RoutingEvent::RouteCreated {
            source_track_guid,
            route,
            ..
        } => {
            suppression.suppress(SuppressionKey::routing(
                source_track_guid,
                &format!("created:{}", route.index),
            ));
            let local_source = resolve_guid(source_track_guid);
            if let Some(project) = resolve_project(daw, ctx).await
                && let Ok(Some(track)) = project.tracks().by_guid(&local_source).await
            {
                match route.route_type {
                    RouteType::Send => {
                        if let Some(ref dest_guid) = route.dest_track_guid {
                            let local_dest = resolve_guid(dest_guid);
                            if let Err(e) = track.sends().add_to(&local_dest).await {
                                warn!(
                                    "Failed to create send from {local_source} to {local_dest}: {e}"
                                );
                            }
                        }
                    }
                    RouteType::HardwareOutput => {
                        if let Some(hw_idx) = route.hw_output_index
                            && let Err(e) = track.hardware_outputs().add(hw_idx).await
                        {
                            warn!("Failed to add hardware output {hw_idx}: {e}");
                        }
                    }
                    RouteType::Receive => {
                        // Receives are the inverse of sends — creating a receive on track A
                        // from track B is the same as creating a send on track B to track A.
                        debug!(
                            "Receive creation not directly supported — handled via send creation on source"
                        );
                    }
                }
            }
        }
        RoutingEvent::RouteDeleted {
            source_track_guid,
            route_type,
            route_index,
            ..
        } => {
            suppression.suppress(SuppressionKey::routing(
                source_track_guid,
                &format!("deleted:{route_index}"),
            ));
            apply_route_mutation(
                daw,
                ctx,
                source_track_guid,
                *route_type,
                *route_index,
                |handle| Box::pin(async move { handle.remove().await }),
            )
            .await;
        }
        RoutingEvent::ParentSendChanged {
            track_guid,
            enabled,
            ..
        } => {
            suppression.suppress(SuppressionKey::routing(track_guid, "parent_send"));
            let local_guid = resolve_guid(track_guid);
            if let Some(project) = resolve_project(daw, ctx).await
                && let Ok(Some(track)) = project.tracks().by_guid(&local_guid).await
                && let Err(e) = track.set_parent_send(*enabled).await
            {
                warn!("Failed to set parent send on {local_guid}: {e}");
            }
        }
    }
}

/// Helper to resolve a route handle and apply a mutation.
async fn apply_route_mutation<F>(
    daw: &Daw,
    ctx: &ProjectContext,
    track_guid: &str,
    route_type: RouteType,
    route_index: u32,
    mutation: F,
) where
    F: FnOnce(
        daw::rpc::RouteHandle,
    )
        -> std::pin::Pin<Box<dyn std::future::Future<Output = daw::rpc::Result<()>> + Send>>,
{
    let local_guid = resolve_guid(track_guid);
    let project = match resolve_project(daw, ctx).await {
        Some(p) => p,
        None => return,
    };
    let track = match project.tracks().by_guid(&local_guid).await {
        Ok(Some(t)) => t,
        Ok(None) => {
            debug!("Track {track_guid} not found for route mutation");
            return;
        }
        Err(e) => {
            warn!("Failed to resolve track {track_guid} for route: {e}");
            return;
        }
    };
    let route_handle = match route_type {
        RouteType::Send => track.sends().by_index(route_index).await,
        RouteType::Receive => track.receives().by_index(route_index).await,
        RouteType::HardwareOutput => track.hardware_outputs().by_index(route_index).await,
    };
    match route_handle {
        Ok(Some(handle)) => {
            if let Err(e) = mutation(handle).await {
                warn!("Route mutation failed on {track_guid}[{route_index}]: {e}");
            }
        }
        Ok(None) => {
            debug!("Route {route_type:?}[{route_index}] not found on track {track_guid}");
        }
        Err(e) => {
            warn!("Failed to resolve route {route_type:?}[{route_index}] on {track_guid}: {e}");
        }
    }
}

// ── Tempo Map ────────────────────────────────────────────────────────────────

async fn apply_tempo_map(
    daw: &Daw,
    _ctx: &ProjectContext,
    project_guid: &str,
    event: &TempoMapEvent,
    suppression: &mut SuppressionSet,
) {
    suppression.suppress(SuppressionKey::tempo_map(project_guid));

    let project = match daw.project(project_guid).await {
        Ok(p) => p,
        Err(_) => return,
    };
    let tempo_map = project.tempo_map();

    match event {
        TempoMapEvent::PointAdded(point) => {
            let pos_secs = point
                .position
                .time
                .as_ref()
                .map(|t| t.as_seconds())
                .unwrap_or(0.0);
            match tempo_map.add_point(pos_secs, point.bpm).await {
                Ok(idx) => {
                    if let Some(ref ts) = point.time_signature {
                        let _ = tempo_map
                            .set_time_signature_at(idx, ts.numerator as i32, ts.denominator as i32)
                            .await;
                    }
                }
                Err(e) => warn!("Failed to add tempo point at {pos_secs:.1}s: {e}"),
            }
        }
        TempoMapEvent::PointRemoved(index) => {
            if let Err(e) = tempo_map.remove_point(*index).await {
                warn!("Failed to remove tempo point at index {index}: {e}");
            }
        }
        TempoMapEvent::PointChanged(point) => {
            // Match by position — find the closest existing point
            let pos_secs = point
                .position
                .time
                .as_ref()
                .map(|t| t.as_seconds())
                .unwrap_or(0.0);
            if let Ok(points) = tempo_map.points().await {
                let closest = points
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| {
                        let a_pos = a
                            .position
                            .time
                            .as_ref()
                            .map(|t| t.as_seconds())
                            .unwrap_or(0.0);
                        let b_pos = b
                            .position
                            .time
                            .as_ref()
                            .map(|t| t.as_seconds())
                            .unwrap_or(0.0);
                        (a_pos - pos_secs)
                            .abs()
                            .partial_cmp(&(b_pos - pos_secs).abs())
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|(i, _)| i as u32);
                if let Some(idx) = closest {
                    let _ = tempo_map.set_tempo_at(idx, point.bpm).await;
                    if let Some(ref ts) = point.time_signature {
                        let _ = tempo_map
                            .set_time_signature_at(idx, ts.numerator as i32, ts.denominator as i32)
                            .await;
                    }
                }
            }
        }
        TempoMapEvent::MapChanged(points) => {
            // Full map replacement — remove all existing points, then add new ones.
            // Keep point 0 (default tempo) and update it, then add the rest.
            if let Ok(existing) = tempo_map.points().await {
                // Remove from end to avoid index shifting
                for i in (1..existing.len()).rev() {
                    let _ = tempo_map.remove_point(i as u32).await;
                }
            }
            // Update point 0 if it exists in the new map
            if let Some(first) = points.first() {
                let _ = tempo_map.set_default_tempo(first.bpm).await;
                if let Some(ref ts) = first.time_signature {
                    let _ = tempo_map
                        .set_default_time_signature(ts.numerator as i32, ts.denominator as i32)
                        .await;
                }
            }
            // Add remaining points
            for point in points.iter().skip(1) {
                let pos_secs = point
                    .position
                    .time
                    .as_ref()
                    .map(|t| t.as_seconds())
                    .unwrap_or(0.0);
                if let Ok(idx) = tempo_map.add_point(pos_secs, point.bpm).await
                    && let Some(ref ts) = point.time_signature
                {
                    let _ = tempo_map
                        .set_time_signature_at(idx, ts.numerator as i32, ts.denominator as i32)
                        .await;
                }
            }
        }
    }
}

// ── Marker ───────────────────────────────────────────────────────────────────

async fn apply_marker(
    daw: &Daw,
    _ctx: &ProjectContext,
    project_guid: &str,
    event: &MarkerEvent,
    suppression: &mut SuppressionSet,
) {
    let project = match daw.project(project_guid).await {
        Ok(p) => p,
        Err(_) => return,
    };
    let markers = project.markers();

    match event {
        // A renumbering is REAPER's bookkeeping, not an edit. The
        // marker is where it was, called what it was called; only the
        // number moved, and the peer's numbers were never ours anyway
        // — they go through `insert_marker_mapping`. Applying this
        // would move a local marker for a change nobody made.
        MarkerEvent::Renumbered { .. } => {}
        MarkerEvent::Added(marker) => {
            let pos = marker
                .position
                .time
                .as_ref()
                .map(|t| t.as_seconds())
                .unwrap_or(0.0);
            match markers.add(pos, &marker.name).await {
                Ok(local_id) => {
                    if let Some(remote_id) = marker.id {
                        suppression.suppress(SuppressionKey::marker(project_guid, local_id));
                        insert_marker_mapping(project_guid, remote_id, local_id);
                    }
                }
                Err(e) => warn!("Failed to add marker '{}': {e}", marker.name),
            }
        }
        MarkerEvent::Changed(marker) => {
            if let Some(remote_id) = marker.id {
                let local_id = resolve_marker_id(project_guid, remote_id);
                suppression.suppress(SuppressionKey::marker(project_guid, local_id));
                let pos = marker
                    .position
                    .time
                    .as_ref()
                    .map(|t| t.as_seconds())
                    .unwrap_or(0.0);
                let _ = markers.move_to(local_id, pos).await;
                let _ = markers.rename(local_id, &marker.name).await;
                if let Some(color) = marker.color {
                    let _ = markers.set_color(local_id, color).await;
                }
            }
        }
        MarkerEvent::Removed(remote_id) => {
            let local_id = resolve_marker_id(project_guid, *remote_id);
            suppression.suppress(SuppressionKey::marker(project_guid, local_id));
            let _ = markers.remove(local_id).await;
            remove_marker_mapping(project_guid, *remote_id);
        }
        MarkerEvent::MarkersChanged(new_markers) => {
            // Bulk marker replacement — remove all existing, then add new ones
            if let Ok(existing) = markers.all().await {
                for m in existing.iter().rev() {
                    if let Some(id) = m.id {
                        suppression.suppress(SuppressionKey::marker(project_guid, id));
                        let _ = markers.remove(id).await;
                    }
                }
            }
            for marker in new_markers {
                let pos = marker
                    .position
                    .time
                    .as_ref()
                    .map(|t| t.as_seconds())
                    .unwrap_or(0.0);
                let _ = markers.add(pos, &marker.name).await;
            }
        }
    }
}

// ── Region ───────────────────────────────────────────────────────────────────

async fn apply_region(
    daw: &Daw,
    _ctx: &ProjectContext,
    project_guid: &str,
    event: &RegionEvent,
    suppression: &mut SuppressionSet,
) {
    let project = match daw.project(project_guid).await {
        Ok(p) => p,
        Err(_) => return,
    };
    let regions = project.regions();

    match event {
        // Same as the marker case: bookkeeping, not an edit.
        RegionEvent::Renumbered { .. } => {}
        RegionEvent::Added(region) => {
            if let Some(id) = region.id {
                suppression.suppress(SuppressionKey::region(project_guid, id));
            }
            let start = region.time_range.start_seconds();
            let end = region.time_range.end_seconds();
            if let Err(e) = regions.add(start, end, &region.name).await {
                warn!("Failed to add region '{}': {e}", region.name);
            }
        }
        RegionEvent::Changed(region) => {
            if let Some(id) = region.id {
                suppression.suppress(SuppressionKey::region(project_guid, id));
                let start = region.time_range.start_seconds();
                let end = region.time_range.end_seconds();
                let _ = regions.set_bounds(id, start, end).await;
                let _ = regions.rename(id, &region.name).await;
                if let Some(color) = region.color {
                    let _ = regions.set_color(id, color).await;
                }
            }
        }
        RegionEvent::Removed(id) => {
            suppression.suppress(SuppressionKey::region(project_guid, *id));
            let _ = regions.remove(*id).await;
        }
        RegionEvent::RegionsChanged(new_regions) => {
            // Bulk region replacement — remove all existing, then add new ones
            if let Ok(existing) = regions.all().await {
                for r in existing.iter().rev() {
                    if let Some(id) = r.id {
                        suppression.suppress(SuppressionKey::region(project_guid, id));
                        let _ = regions.remove(id).await;
                    }
                }
            }
            for region in new_regions {
                let start = region.time_range.start_seconds();
                let end = region.time_range.end_seconds();
                let _ = regions.add(start, end, &region.name).await;
            }
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

async fn resolve_project(daw: &Daw, ctx: &ProjectContext) -> Option<daw::rpc::Project> {
    match ctx {
        ProjectContext::Project(guid) => match daw.project(guid.as_str()).await {
            Ok(p) => Some(p),
            Err(e) => {
                debug!("Project {guid} not found locally: {e}");
                None
            }
        },
        ProjectContext::Current => match daw.current_project().await {
            Ok(p) => Some(p),
            Err(e) => {
                warn!("Failed to get current project: {e}");
                None
            }
        },
    }
}

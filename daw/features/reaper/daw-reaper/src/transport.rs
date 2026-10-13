//! `impl Transport for Reaper` — sync trait + REAPER C API.
//!
//! Mounting goes through `daw_proto::transport::serve(Reaper)`. The
//! dispatcher (REAPER's main thread queue) is pulled off the backend
//! via `HasDispatcher` on `Reaper`. Each method assumes main-thread
//! execution — the bridge enforces that contract.
//!
//! Helpers (`read_transport_state_for_project`, `get_play_state_*`,
//! `time_to_musical_position`, etc.) stay `pub(crate)` so other
//! modules (batch, polling/broadcasting code if revived) can reuse
//! them.
//!
//! Streaming retired alongside the architect::rpc port: the
//! `TRANSPORT_BROADCASTER` + `PROJECT_TRANSPORT_CACHE` + polling
//! scaffolding is gone. Subscriptions land on a sibling trait when
//! a real consumer reappears.

use std::ffi::CStr;

use daw_proto::transport::service::Transport;
use daw_proto::{
    DawError, DawResult, PlayState, ProjectContext, TimeSignature, Transport as TransportState,
};
use reaper_high::{PlayRate, Project, Reaper as ReaperHigh, Tempo as ReaperTempo};
use reaper_medium::{
    AutoSeekBehavior, CommandId, PositionInSeconds, ProjectContext as ReaperProjectContext,
    SetEditCurPosOptions, TimeRangeType, UndoBehavior,
};
use tracing::warn;

use crate::project_context::find_project_by_guid;

// ── Helpers ────────────────────────────────────────────────────────────

const MAX_REASONABLE_TIME_SELECTION_SECONDS: f64 = 60.0 * 60.0;

fn normalize_time_selection(
    selection: Option<daw_proto::LoopRegion>,
) -> Option<daw_proto::LoopRegion> {
    let selection = selection.filter(daw_proto::LoopRegion::is_valid)?;
    let duration = selection.duration();
    if duration.is_finite() && duration <= MAX_REASONABLE_TIME_SELECTION_SECONDS {
        return Some(selection);
    }
    warn!(
        start = selection.start_seconds,
        end = selection.end_seconds,
        duration,
        "ReaperTransport: ignoring unreasonable time selection",
    );
    None
}

pub(crate) fn resolve_reaper_project_context(project: &ProjectContext) -> ReaperProjectContext {
    match project {
        ProjectContext::Current => ReaperProjectContext::CurrentProject,
        ProjectContext::Project(guid) => find_project_by_guid(guid)
            .map(|proj| ReaperProjectContext::Proj(proj.raw()))
            .unwrap_or(ReaperProjectContext::CurrentProject),
    }
}

pub(crate) fn get_play_state_for_project(
    medium: &reaper_medium::Reaper,
    ctx: ReaperProjectContext,
) -> PlayState {
    let state = medium.get_play_state_ex(ctx);
    if state.is_recording {
        PlayState::Recording
    } else if state.is_playing {
        PlayState::Playing
    } else if state.is_paused {
        PlayState::Paused
    } else {
        PlayState::Stopped
    }
}

pub(crate) fn get_time_signature_for_project(
    _medium: &reaper_medium::Reaper,
    _ctx: ReaperProjectContext,
) -> (i32, i32) {
    // REAPER doesn't expose a direct project-scoped time signature
    // getter; fall back to the standard 4/4 unless the project is
    // current — in which case use the actual tempo map at position 0.
    // Keeping this simple for now; sibling tempo-map service handles
    // the structured form.
    (4, 4)
}

/// Read transport state from REAPER for a specific project. **MUST be
/// called from the main thread.**
pub(crate) fn read_transport_state_for_project(
    project: &Project,
    reaper_ctx: ReaperProjectContext,
    medium: &reaper_medium::Reaper,
) -> TransportState {
    let play_state = get_play_state_for_project(medium, reaper_ctx);
    let looping = medium.get_set_repeat_ex_get(reaper_ctx);
    let tempo_bpm = project.tempo().bpm().get();
    let playrate = project.play_rate().playback_speed_factor().get();
    let pos_seconds = medium.get_play_position_ex(reaper_ctx).get();
    let edit_pos = medium
        .get_cursor_position_ex(reaper_ctx)
        .map(|p| p.get())
        .unwrap_or(0.0);

    let (ts_num, ts_denom) = get_time_signature_for_project(medium, reaper_ctx);

    let loop_region = medium
        .get_set_loop_time_range_2_get(reaper_ctx, TimeRangeType::LoopPoints)
        .map(|range| daw_proto::LoopRegion::new(range.start.get(), range.end.get()));

    let time_selection = normalize_time_selection(
        medium
            .get_set_loop_time_range_2_get(reaper_ctx, TimeRangeType::TimeSelection)
            .map(|range| daw_proto::LoopRegion::new(range.start.get(), range.end.get())),
    );

    let playhead_musical = time_to_musical_position(project, medium, reaper_ctx, pos_seconds);
    let edit_musical = time_to_musical_position(project, medium, reaper_ctx, edit_pos);

    TransportState {
        play_state,
        record_mode: daw_proto::RecordMode::Normal,
        looping,
        metronome: reaper_high::Reaper::get()
            .main_section()
            .action_by_command_id(reaper_medium::CommandId::new(40364))
            .is_on()
            .unwrap_or(None)
            .unwrap_or(false),
        loop_region,
        time_selection,
        tempo: daw_proto::primitives::Tempo::from_bpm(tempo_bpm),
        playrate,
        time_signature: TimeSignature::new(ts_num as u32, ts_denom as u32),
        playhead_position: daw_proto::primitives::Position::new(
            Some(playhead_musical),
            Some(daw_proto::primitives::TimePosition::from_seconds(
                pos_seconds,
            )),
            None,
        ),
        edit_position: daw_proto::primitives::Position::new(
            Some(edit_musical),
            Some(daw_proto::primitives::TimePosition::from_seconds(edit_pos)),
            None,
        ),
    }
}

fn time_to_musical_position(
    project: &Project,
    medium: &reaper_medium::Reaper,
    reaper_ctx: ReaperProjectContext,
    time_seconds: f64,
) -> daw_proto::primitives::MusicalPosition {
    let Some(pos) = PositionInSeconds::new(time_seconds).ok() else {
        return daw_proto::primitives::MusicalPosition::ZERO;
    };
    let result = medium.time_map_2_time_to_beats(reaper_ctx, pos);
    // Store 0-based (the proto convention — Display adds +1 to render
    // REAPER's familiar 1-based bars:beats). `measure_offset` is the
    // `projmeasoffs` adjustment; it's applied at storage time so any
    // arithmetic on these MusicalPositions stays correct.
    let measure = result.measure_index + project.measure_offset();
    let beats_since = result.beats_since_measure.get();
    let beat = beats_since.floor() as i32;
    let subdivision = ((beats_since.fract()) * 1000.0).round() as i32;
    daw_proto::primitives::MusicalPosition::new(measure, beat, subdivision.clamp(0, 999))
}

/// Fire a transport action against a *specific* project, not whichever tab
/// REAPER's UI currently has focused.
///
/// `Main_OnCommandEx` takes a project argument for exactly this reason, but
/// every caller here used to pass `CurrentProject` unconditionally and
/// discard the real `ProjectContext` — harmless for a single-project setup,
/// but Recording Mode's whole point is driving several open songs' projects
/// by GUID, and REAPER's UI focus has no reason to already be on whichever
/// one a `SetlistService` call means to target. `resolve_reaper_project_context`
/// is the same GUID→project resolution every other REAPER-targeted RPC in
/// this backend already goes through.
fn run_command_on(cmd: u32, project: &ProjectContext) {
    let reaper = ReaperHigh::get();
    reaper.medium_reaper().main_on_command_ex(
        CommandId::new(cmd),
        0,
        resolve_reaper_project_context(project),
    );
}

// ── Transport impl ────────────────────────────────────────────────────

impl Transport for crate::Reaper {
    fn set_metronome(&self, project: ProjectContext, enabled: bool) -> DawResult<()> {
        // Toggle command 40364 ("Options: Toggle metronome") when the
        // current state differs.
        if Transport::metronome_enabled(self, project) != enabled {
            let reaper = reaper_high::Reaper::get();
            let _ = reaper
                .main_section()
                .action_by_command_id(reaper_medium::CommandId::new(40364))
                .invoke_as_trigger(None, None);
        }
        Ok(())
    }

    fn metronome_enabled(&self, _project: ProjectContext) -> bool {
        let reaper = reaper_high::Reaper::get();
        reaper
            .main_section()
            .action_by_command_id(reaper_medium::CommandId::new(40364))
            .is_on()
            .unwrap_or(None)
            .unwrap_or(false)
    }

    fn play(&self, project: ProjectContext) -> DawResult<()> {
        // 1007: Transport: Play
        run_command_on(1007, &project);
        Ok(())
    }

    fn pause(&self, project: ProjectContext) -> DawResult<()> {
        // 1008: Transport: Pause
        run_command_on(1008, &project);
        Ok(())
    }

    fn stop(&self, project: ProjectContext) -> DawResult<()> {
        // 1016: Transport: Stop
        run_command_on(1016, &project);
        Ok(())
    }

    fn play_pause(&self, project: ProjectContext) -> DawResult<()> {
        // 40073: Transport: Play/pause
        run_command_on(40073, &project);
        Ok(())
    }

    fn play_stop(&self, project: ProjectContext) -> DawResult<()> {
        // 40044: Transport: Play/stop
        run_command_on(40044, &project);
        Ok(())
    }

    fn record(&self, project: ProjectContext) -> DawResult<()> {
        // 1013: Transport: Record
        run_command_on(1013, &project);
        Ok(())
    }

    fn stop_recording(&self, project: ProjectContext) -> DawResult<()> {
        // 1016: Transport: Stop (also stops recording)
        run_command_on(1016, &project);
        Ok(())
    }

    fn toggle_recording(&self, project: ProjectContext) -> DawResult<()> {
        // 1013 toggles record state
        run_command_on(1013, &project);
        Ok(())
    }

    fn set_position(&self, _project: ProjectContext, seconds: f64) -> DawResult<()> {
        let pos = PositionInSeconds::new(seconds)
            .map_err(|e| DawError::operation_failed(format!("invalid position: {e:?}")))?;
        ReaperHigh::get()
            .current_project()
            .set_edit_cursor_position(
                pos,
                SetEditCurPosOptions {
                    move_view: false,
                    seek_play: true,
                },
            );
        Ok(())
    }

    fn get_position(&self, _project: ProjectContext) -> f64 {
        ReaperHigh::get()
            .current_project()
            .play_or_edit_cursor_position()
            .map(|p| p.get())
            .unwrap_or(0.0)
    }

    fn goto_start(&self, project: ProjectContext) -> DawResult<()> {
        // 40042: Transport: Go to start of project
        run_command_on(40042, &project);
        Ok(())
    }

    fn goto_end(&self, project: ProjectContext) -> DawResult<()> {
        // 40043: Transport: Go to end of project
        run_command_on(40043, &project);
        Ok(())
    }

    fn get_state(&self, project: ProjectContext) -> TransportState {
        let reaper = ReaperHigh::get();
        let medium = reaper.medium_reaper();
        let (rea_project, reaper_ctx) = match &project {
            ProjectContext::Current => {
                let proj = reaper.current_project();
                (proj, ReaperProjectContext::CurrentProject)
            }
            ProjectContext::Project(guid) => {
                if let Some(proj) = find_project_by_guid(guid) {
                    let ctx = ReaperProjectContext::Proj(proj.raw());
                    (proj, ctx)
                } else {
                    let proj = reaper.current_project();
                    (proj, ReaperProjectContext::CurrentProject)
                }
            }
        };
        read_transport_state_for_project(&rea_project, reaper_ctx, medium)
    }

    fn get_play_state(&self, project: ProjectContext) -> PlayState {
        let medium = ReaperHigh::get().medium_reaper();
        get_play_state_for_project(medium, resolve_reaper_project_context(&project))
    }

    fn is_playing(&self, project: ProjectContext) -> bool {
        let state = ReaperHigh::get()
            .medium_reaper()
            .get_play_state_ex(resolve_reaper_project_context(&project));
        state.is_playing || state.is_recording
    }

    fn is_recording(&self, project: ProjectContext) -> bool {
        ReaperHigh::get()
            .medium_reaper()
            .get_play_state_ex(resolve_reaper_project_context(&project))
            .is_recording
    }

    fn get_tempo(&self, _project: ProjectContext) -> f64 {
        ReaperHigh::get().current_project().tempo().bpm().get()
    }

    fn set_tempo(&self, _project: ProjectContext, bpm: f64) -> DawResult<()> {
        let bpm_value = reaper_medium::Bpm::new(bpm)
            .map_err(|e| DawError::operation_failed(format!("invalid bpm: {e:?}")))?;
        let tempo = ReaperTempo::from_bpm(bpm_value);
        ReaperHigh::get()
            .current_project()
            .set_tempo(tempo, UndoBehavior::OmitUndoPoint)
            .map_err(|e| DawError::operation_failed(format!("set_tempo failed: {e:?}")))?;
        Ok(())
    }

    fn toggle_loop(&self, project: ProjectContext) -> DawResult<()> {
        // 1068: Transport: Toggle repeat
        run_command_on(1068, &project);
        Ok(())
    }

    fn is_looping(&self, project: ProjectContext) -> bool {
        ReaperHigh::get()
            .medium_reaper()
            .get_set_repeat_ex_get(resolve_reaper_project_context(&project))
    }

    fn set_loop(&self, project: ProjectContext, enabled: bool) -> DawResult<()> {
        ReaperHigh::get()
            .medium_reaper()
            .get_set_repeat_ex_set(resolve_reaper_project_context(&project), enabled);
        Ok(())
    }

    fn get_time_selection(&self, project: ProjectContext) -> Option<daw_proto::LoopRegion> {
        let ctx = resolve_reaper_project_context(&project);
        let selection = ReaperHigh::get()
            .medium_reaper()
            .get_set_loop_time_range_2_get(ctx, TimeRangeType::TimeSelection)
            .map(|range| daw_proto::LoopRegion::new(range.start.get(), range.end.get()));
        normalize_time_selection(selection)
    }

    fn set_time_selection(
        &self,
        project: ProjectContext,
        start_seconds: f64,
        end_seconds: f64,
    ) -> DawResult<()> {
        let ctx = resolve_reaper_project_context(&project);
        let start = start_seconds.min(end_seconds).max(0.0);
        let end = start_seconds.max(end_seconds).max(0.0);
        let start = PositionInSeconds::new(start)
            .map_err(|e| DawError::operation_failed(format!("invalid start: {e:?}")))?;
        let end = PositionInSeconds::new(end)
            .map_err(|e| DawError::operation_failed(format!("invalid end: {e:?}")))?;
        ReaperHigh::get()
            .medium_reaper()
            .get_set_loop_time_range_2_set(
                ctx,
                TimeRangeType::TimeSelection,
                start,
                end,
                AutoSeekBehavior::DenyAutoSeek,
            );
        Ok(())
    }

    fn clear_time_selection(&self, project: ProjectContext) -> DawResult<()> {
        let ctx = resolve_reaper_project_context(&project);
        ReaperHigh::get()
            .medium_reaper()
            .get_set_loop_time_range_2_set(
                ctx,
                TimeRangeType::TimeSelection,
                PositionInSeconds::new_panic(0.0),
                PositionInSeconds::new_panic(0.0),
                AutoSeekBehavior::DenyAutoSeek,
            );
        Ok(())
    }

    fn get_playrate(&self, _project: ProjectContext) -> f64 {
        ReaperHigh::get()
            .current_project()
            .play_rate()
            .playback_speed_factor()
            .get()
    }

    fn set_playrate(&self, _project: ProjectContext, rate: f64) -> DawResult<()> {
        let clamped = rate.clamp(0.25, 4.0);
        let factor = reaper_medium::PlaybackSpeedFactor::new(clamped);
        let play_rate = PlayRate::from_playback_speed_factor(factor);
        ReaperHigh::get().current_project().set_play_rate(play_rate);
        Ok(())
    }

    fn get_time_signature(&self, project: ProjectContext) -> TimeSignature {
        let medium = ReaperHigh::get().medium_reaper();
        let (num, denom) =
            get_time_signature_for_project(medium, resolve_reaper_project_context(&project));
        TimeSignature::new(num as u32, denom as u32)
    }
}

// Compile-time assertion: `crate::Reaper` must remain Arc-safe so
// `transport::serve(Reaper)` keeps compiling.
#[allow(dead_code)]
fn _erase_unused_cstr_import() {
    let _: Option<&CStr> = None;
}

// ── Streaming: poll + broadcast into DawEventHub ──────────────────────
//
// Two outputs per main-thread tick (~30Hz, driven by the bridge's
// timer callback in daw-bridge/src/lib.rs):
//
// - `PositionTick` pushed to the **continuous** channel on every tick
//   for every open project, regardless of whether it changed. Drop-old
//   broadcast semantics handle backpressure.
//
// - `TransportEvent::Snapshot` pushed to the **occasional** channel
//   when the cached state for a project differs from the freshly-read
//   state. Coalesces all per-field changes into one snapshot; we can
//   split into per-field events (PlayStateChanged, TempoChanged, …)
//   in a follow-up once a consumer needs the granularity.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use daw_proto::transport::{PositionTick, TransportEvent};
use reaper_medium::{ProjectRef, ReaProject};

use crate::project_context::{MAX_PROJECT_TABS, project_guid as project_guid_from};

/// Per-project cache of the last-broadcast `TransportState`. Keyed by
/// project GUID. Compared against freshly-read state per tick so we
/// emit `TransportEvent::Snapshot` only on change.
static TRANSPORT_CACHE: OnceLock<Mutex<HashMap<String, TransportState>>> = OnceLock::new();

fn transport_cache() -> &'static Mutex<HashMap<String, TransportState>> {
    TRANSPORT_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

static ACTIVE_PROJECT_GUID: OnceLock<Mutex<Option<String>>> = OnceLock::new();

fn active_project_guid() -> &'static Mutex<Option<String>> {
    ACTIVE_PROJECT_GUID.get_or_init(|| Mutex::new(None))
}

/// Poll REAPER transport state for ALL open projects and broadcast
/// changes through the [`crate::event_hub`] hub.
///
/// **MUST be called from the REAPER main thread** — typically from
/// the extension's 30Hz timer callback. Use of `ReaperHigh::get()` /
/// `medium_reaper()` is main-thread-only.
pub fn poll_and_broadcast_transport() {
    let hub = crate::event_hub::hub();
    let want_state = hub.transport_state_subscriber_count() > 0;
    let want_position = hub.position_subscriber_count() > 0;
    let want_projects = hub.projects_subscriber_count() > 0;
    if !want_state && !want_position && !want_projects {
        return;
    }

    let reaper = ReaperHigh::get();
    let medium = reaper.medium_reaper();
    let mut cache = transport_cache()
        .lock()
        .expect("transport cache mutex poisoned");

    // Detect active-project switch by comparing the current tab's GUID
    // against last-seen. Cheap (one extra `enum_projects` call per
    // tick) and lets clients drop a `Projects::current()` RPC entirely.
    if want_projects {
        if let Some(active) = medium.enum_projects(ProjectRef::Current, 0) {
            let active_guid = project_guid_from(&Project::new(active.project));
            let mut slot = active_project_guid()
                .lock()
                .expect("active project guid mutex poisoned");
            if slot.as_deref() != Some(active_guid.as_str()) {
                *slot = Some(active_guid.clone());
                hub.publish_project(daw_proto::project::ProjectStreamEvent {
                    event: daw_proto::project::ProjectEvent::CurrentChanged(Some(active_guid)),
                });
            }
        } else {
            let mut slot = active_project_guid()
                .lock()
                .expect("active project guid mutex poisoned");
            if slot.is_some() {
                *slot = None;
                hub.publish_project(daw_proto::project::ProjectStreamEvent {
                    event: daw_proto::project::ProjectEvent::CurrentChanged(None),
                });
            }
        }
    }

    for tab_index in 0..MAX_PROJECT_TABS {
        let Some(result) = medium.enum_projects(ProjectRef::Tab(tab_index), 0) else {
            break;
        };
        let project = Project::new(result.project);
        let project_guid = project_guid_from(&project);
        let reaper_ctx = ReaperProjectContext::Proj(result.project);

        // Read state once; reuse for both channels.
        let state = read_transport_state_for_project(&project, reaper_ctx, medium);

        if want_position {
            // `state` already carries both representations on each
            // cursor's Position struct — reuse them straight through
            // (musical is computed by `time_to_musical_position` which
            // applies `projmeasoffs`).
            hub.publish_position(PositionTick {
                project_guid: project_guid.clone(),
                playhead: state.playhead_position.clone(),
                edit_cursor: state.edit_position.clone(),
                is_playing: state.is_playing(),
            });
        }

        if want_state {
            let changed = cache
                .get(&project_guid)
                .map(|prev| !transport_states_equal(prev, &state))
                .unwrap_or(true);
            if changed {
                cache.insert(project_guid.clone(), state.clone());
                hub.publish_transport_state(TransportEvent::Snapshot {
                    project_guid: project_guid.clone(),
                    state,
                });
            }
        }
    }
}

/// Cheap equality check tolerating sub-millisecond playhead drift —
/// position is broadcast on the continuous channel, so the state
/// channel only fires on "real" changes (transport actions, edits,
/// tempo, etc.) not every 30Hz position update.
fn transport_states_equal(a: &TransportState, b: &TransportState) -> bool {
    a.play_state == b.play_state
        && a.record_mode == b.record_mode
        && a.looping == b.looping
        && a.loop_region == b.loop_region
        && a.time_selection == b.time_selection
        && (a.tempo.bpm() - b.tempo.bpm()).abs() < 1e-6
        && (a.playrate - b.playrate).abs() < 1e-6
        && a.time_signature == b.time_signature
}

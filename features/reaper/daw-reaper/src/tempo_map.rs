//! `impl TempoMap for Reaper` — sync trait + REAPER C API.
//!
//! Mounting goes through `daw_proto::tempo_map::serve(Reaper)`. The
//! dispatcher (REAPER's main thread queue) handles threading via
//! `HasDispatcher`. Each method assumes main-thread execution.
//!
//! Streaming (subscribe_tempo_map) + the broadcaster + cache + polling
//! scaffolding retired alongside the architect::rpc port — sibling-
//! trait territory if revived.
//!
//! Helpers `time_to_qn_on_main_thread`, `qn_to_time_on_main_thread`,
//! and `get_tempo_and_time_sig_at_on_main_thread` stay public for
//! callers that hold a main-thread proof (batch sync dispatcher etc).

use daw_control::lock::LockExt;
use daw_proto::TempoMap;
use daw_proto::tempo_map::TempoMapEvent;
use daw_proto::{
    DawError, DawResult, Position, ProjectContext, TempoPoint, TimePosition, TimeSignature,
};
use reaper_high::Reaper as ReaperHigh;
use reaper_medium::{MeasureMode, ProjectContext as ReaperProjectContext};

use crate::project_context::resolve_project_context;
use crate::safe_wrappers::tempo as sw;
use crate::safe_wrappers::time_map as tw;

// ── Public sync helpers ────────────────────────────────────────────────

pub fn time_to_qn_on_main_thread(seconds: f64) -> f64 {
    let low = ReaperHigh::get().medium_reaper().low();
    tw::time_to_qn(low, ReaperProjectContext::CurrentProject, seconds)
}

pub fn qn_to_time_on_main_thread(qn: f64) -> f64 {
    let low = ReaperHigh::get().medium_reaper().low();
    tw::qn_to_time_current(low, qn)
}

pub fn get_tempo_and_time_sig_at_on_main_thread(seconds: f64) -> (f64, i32, i32) {
    let low = ReaperHigh::get().medium_reaper().low();
    let ts = tw::get_time_sig_at_time(low, ReaperProjectContext::CurrentProject, seconds);
    (ts.tempo, ts.num, ts.denom)
}

/// Stub kept so plugin_services + daw-bridge can call it during init
/// without touching the broadcaster-retired surface. The architect::rpc
/// port retired streaming; this is a no-op until a sibling trait
/// brings it back.
pub fn init_tempo_map_broadcaster() {}

/// Poll REAPER tempo map for every open project, emit
/// `TempoMapEvent`s for changes. **Main thread only.**
pub fn poll_and_broadcast_tempo_map() {
    use reaper_medium::ProjectRef;
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    use crate::project_context::{MAX_PROJECT_TABS, project_guid};

    static TEMPO_CACHE: OnceLock<Mutex<HashMap<String, Vec<TempoPoint>>>> = OnceLock::new();
    fn cache() -> &'static Mutex<HashMap<String, Vec<TempoPoint>>> {
        TEMPO_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
    }

    let hub = crate::event_hub::hub();
    if hub.tempo_map_subscriber_count() == 0 {
        return;
    }

    let reaper = ReaperHigh::get();
    let medium = reaper.medium_reaper();
    let mut cache = cache().lock_recoverable("tempo_map::cache");

    let mut seen_projects: Vec<String> = Vec::new();

    for tab_index in 0..MAX_PROJECT_TABS {
        let Some(result) = medium.enum_projects(ProjectRef::Tab(tab_index), 0) else {
            break;
        };
        let project = reaper_high::Project::new(result.project);
        let project_guid_str = project_guid(&project);
        seen_projects.push(project_guid_str.clone());

        let project_ctx = ProjectContext::Project(project_guid_str.clone());
        let fresh: Vec<TempoPoint> =
            daw_proto::TempoMap::get_tempo_points(&crate::Reaper, project_ctx);

        let prev = cache.entry(project_guid_str.clone()).or_default();
        if *prev != fresh {
            // Tempo map changes are typically wholesale — emit
            // MapChanged with the whole list. Fine-grained
            // PointAdded/PointRemoved/PointChanged events would
            // need a positional diff that isn't worth the
            // complexity until a consumer needs it.
            hub.publish_tempo_map(daw_proto::tempo_map::TempoMapStreamEvent {
                project_guid: project_guid_str.clone(),
                event: TempoMapEvent::MapChanged(fresh.clone()),
            });
            *prev = fresh;
        }
    }

    cache.retain(|guid, _| seen_projects.iter().any(|seen| seen == guid));
}

// ── Helpers ────────────────────────────────────────────────────────────

fn marker_to_point(m: &sw::TempoMarkerRaw) -> TempoPoint {
    let time_sig = if m.timesig_num > 0 && m.timesig_denom > 0 {
        Some(TimeSignature::new(
            m.timesig_num as u32,
            m.timesig_denom as u32,
        ))
    } else {
        None
    };

    TempoPoint {
        position: Position::from_time(TimePosition::from_seconds(m.timepos)),
        bpm: m.bpm,
        time_signature: time_sig,
        shape: None,
        bezier_tension: None,
        selected: None,
        linear: Some(m.lineartempo),
    }
}

// ── TempoMap impl ─────────────────────────────────────────────────────

impl TempoMap for crate::Reaper {
    fn get_tempo_points(&self, _project: ProjectContext) -> Vec<TempoPoint> {
        let reaper = ReaperHigh::get();
        let medium = reaper.medium_reaper();
        let low = medium.low();
        let count = medium.count_tempo_time_sig_markers(ReaperProjectContext::CurrentProject);
        let mut points = Vec::with_capacity(count as usize);
        for i in 0..count {
            if let Some(m) =
                sw::get_tempo_marker(low, ReaperProjectContext::CurrentProject, i as i32)
            {
                points.push(marker_to_point(&m));
            }
        }
        points
    }

    fn get_tempo_point(&self, _project: ProjectContext, index: u32) -> Option<TempoPoint> {
        let low = ReaperHigh::get().medium_reaper().low();
        sw::get_tempo_marker(low, ReaperProjectContext::CurrentProject, index as i32)
            .map(|m| marker_to_point(&m))
    }

    fn tempo_point_count(&self, _project: ProjectContext) -> u32 {
        ReaperHigh::get()
            .medium_reaper()
            .count_tempo_time_sig_markers(ReaperProjectContext::CurrentProject)
    }

    fn get_tempo_at(&self, _project: ProjectContext, seconds: f64) -> f64 {
        let reaper = ReaperHigh::get();
        let medium = reaper.medium_reaper();
        if let Ok(pos) = reaper_medium::PositionInSeconds::new(seconds) {
            medium
                .time_map_2_get_divided_bpm_at_time(ReaperProjectContext::CurrentProject, pos)
                .get()
        } else {
            reaper.current_project().tempo().bpm().get()
        }
    }

    fn get_time_signature_at(&self, _project: ProjectContext, seconds: f64) -> (i32, i32) {
        let medium = ReaperHigh::get().medium_reaper();
        if let Ok(pos) = reaper_medium::PositionInSeconds::new(seconds) {
            let beat_info =
                medium.time_map_2_time_to_beats(ReaperProjectContext::CurrentProject, pos);
            (
                beat_info.time_signature.numerator.get() as i32,
                beat_info.time_signature.denominator.get() as i32,
            )
        } else {
            (4, 4)
        }
    }

    fn time_to_musical(&self, project: ProjectContext, seconds: f64) -> (i32, i32, f64) {
        let medium = ReaperHigh::get().medium_reaper();
        let ctx = resolve_project_context(&project);
        if let Ok(pos) = reaper_medium::PositionInSeconds::new(seconds) {
            let result = medium.time_map_2_time_to_beats(ctx, pos);
            let measure = result.measure_index + 1;
            let beats_since = result.beats_since_measure.get();
            let beat = beats_since.floor() as i32 + 1;
            let fraction = beats_since.fract();
            (measure, beat, fraction)
        } else {
            (1, 1, 0.0)
        }
    }

    fn musical_to_time(
        &self,
        project: ProjectContext,
        measure: i32,
        beat: i32,
        fraction: f64,
    ) -> f64 {
        let medium = ReaperHigh::get().medium_reaper();
        let ctx = resolve_project_context(&project);
        let measure_0based = (measure - 1).max(0);
        let beat_0based = (beat - 1).max(0) as f64 + fraction;
        let measure_start = tw::get_measure_info(medium.low(), ctx, measure_0based);
        if let Ok(measure_start_pos) = reaper_medium::PositionInSeconds::new(measure_start) {
            let measure_start_beats = medium
                .time_map_2_time_to_beats(ctx, measure_start_pos)
                .full_beats;
            let absolute_beats = measure_start_beats.get() + beat_0based;
            let Ok(beats) = reaper_medium::PositionInBeats::new(absolute_beats) else {
                return 0.0;
            };
            medium
                .time_map_2_beats_to_time(ctx, MeasureMode::IgnoreMeasure, beats)
                .get()
        } else {
            0.0
        }
    }

    fn add_tempo_point(&self, _project: ProjectContext, seconds: f64, bpm: f64) -> DawResult<u32> {
        let reaper = ReaperHigh::get();
        let low = reaper.medium_reaper().low();
        let ok = sw::set_tempo_marker(
            low,
            ReaperProjectContext::CurrentProject,
            -1,
            seconds,
            -1,
            -1.0,
            bpm,
            0,
            0,
            false,
        );
        if !ok {
            return Err(DawError::operation_failed("add_tempo_point failed"));
        }
        let count = reaper
            .medium_reaper()
            .count_tempo_time_sig_markers(ReaperProjectContext::CurrentProject);
        Ok(count.saturating_sub(1))
    }

    fn remove_tempo_point(&self, _project: ProjectContext, index: u32) -> DawResult<()> {
        let low = ReaperHigh::get().medium_reaper().low();
        sw::delete_tempo_marker(low, ReaperProjectContext::CurrentProject, index as i32);
        Ok(())
    }

    fn set_tempo_at_point(&self, _project: ProjectContext, index: u32, bpm: f64) -> DawResult<()> {
        let low = ReaperHigh::get().medium_reaper().low();
        let m = sw::get_tempo_marker(low, ReaperProjectContext::CurrentProject, index as i32)
            .ok_or_else(|| DawError::not_found("TempoPoint", &index.to_string()))?;
        let ok = sw::set_tempo_marker(
            low,
            ReaperProjectContext::CurrentProject,
            index as i32,
            m.timepos,
            m.measurepos,
            m.beatpos,
            bpm,
            m.timesig_num,
            m.timesig_denom,
            m.lineartempo,
        );
        if !ok {
            return Err(DawError::operation_failed("set_tempo_at_point failed"));
        }
        Ok(())
    }

    fn set_time_signature_at_point(
        &self,
        _project: ProjectContext,
        index: u32,
        numerator: i32,
        denominator: i32,
    ) -> DawResult<()> {
        let low = ReaperHigh::get().medium_reaper().low();
        let m = sw::get_tempo_marker(low, ReaperProjectContext::CurrentProject, index as i32)
            .ok_or_else(|| DawError::not_found("TempoPoint", &index.to_string()))?;
        let ok = sw::set_tempo_marker(
            low,
            ReaperProjectContext::CurrentProject,
            index as i32,
            m.timepos,
            m.measurepos,
            m.beatpos,
            m.bpm,
            numerator,
            denominator,
            m.lineartempo,
        );
        if !ok {
            return Err(DawError::operation_failed(
                "set_time_signature_at_point failed",
            ));
        }
        Ok(())
    }

    fn move_tempo_point(
        &self,
        _project: ProjectContext,
        index: u32,
        seconds: f64,
    ) -> DawResult<()> {
        let low = ReaperHigh::get().medium_reaper().low();
        let m = sw::get_tempo_marker(low, ReaperProjectContext::CurrentProject, index as i32)
            .ok_or_else(|| DawError::not_found("TempoPoint", &index.to_string()))?;
        let ok = sw::set_tempo_marker(
            low,
            ReaperProjectContext::CurrentProject,
            index as i32,
            seconds,
            -1,
            -1.0,
            m.bpm,
            m.timesig_num,
            m.timesig_denom,
            m.lineartempo,
        );
        if !ok {
            return Err(DawError::operation_failed("move_tempo_point failed"));
        }
        Ok(())
    }

    fn get_default_tempo(&self, _project: ProjectContext) -> f64 {
        ReaperHigh::get().current_project().tempo().bpm().get()
    }

    fn set_default_tempo(&self, _project: ProjectContext, bpm: f64) -> DawResult<()> {
        let bpm_value = reaper_medium::Bpm::new(bpm)
            .map_err(|e| DawError::operation_failed(format!("invalid bpm: {e:?}")))?;
        let tempo = reaper_high::Tempo::from_bpm(bpm_value);
        ReaperHigh::get()
            .current_project()
            .set_tempo(tempo, reaper_medium::UndoBehavior::OmitUndoPoint)
            .map_err(|e| DawError::operation_failed(format!("set_default_tempo: {e:?}")))?;
        Ok(())
    }

    fn get_default_time_signature(&self, _project: ProjectContext) -> (i32, i32) {
        let medium = ReaperHigh::get().medium_reaper();
        let measure_info =
            medium.time_map_get_measure_info(ReaperProjectContext::CurrentProject, 0);
        (
            measure_info.time_signature.numerator.get() as i32,
            measure_info.time_signature.denominator.get() as i32,
        )
    }

    fn set_default_time_signature(
        &self,
        _project: ProjectContext,
        numerator: i32,
        denominator: i32,
    ) -> DawResult<()> {
        let reaper = ReaperHigh::get();
        let low = reaper.medium_reaper().low();
        let bpm = reaper.current_project().tempo().bpm().get();
        let count = reaper
            .medium_reaper()
            .count_tempo_time_sig_markers(ReaperProjectContext::CurrentProject);

        let mut updated = false;
        for i in 0..count {
            if let Some(m) =
                sw::get_tempo_marker(low, ReaperProjectContext::CurrentProject, i as i32)
                && m.timepos < 0.001
            {
                sw::set_tempo_marker(
                    low,
                    ReaperProjectContext::CurrentProject,
                    i as i32,
                    0.0,
                    0,
                    0.0,
                    m.bpm,
                    numerator,
                    denominator,
                    m.lineartempo,
                );
                updated = true;
                break;
            }
        }
        if !updated {
            sw::set_tempo_marker(
                low,
                ReaperProjectContext::CurrentProject,
                -1,
                0.0,
                0,
                0.0,
                bpm,
                numerator,
                denominator,
                false,
            );
        }
        Ok(())
    }
}

impl daw_proto::transport::service::TransportStreamSource for crate::Reaper {
    fn events_hub(&self) -> &architect::PubSub<daw_proto::transport::TransportStreamEvent> {
        crate::event_hub::hub().transport_hub()
    }
}

impl daw_proto::tempo_map::TempoMapStreamSource for crate::Reaper {
    fn events_hub(&self) -> &architect::PubSub<daw_proto::tempo_map::TempoMapStreamEvent> {
        crate::event_hub::hub().tempo_map_hub()
    }
}

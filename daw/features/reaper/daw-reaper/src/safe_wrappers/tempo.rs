//! Safe wrappers for REAPER tempo/time-signature marker APIs.

use super::ReaperLow;
use reaper_medium::ProjectContext;

/// Raw tempo marker data returned by `GetTempoTimeSigMarker`.
#[derive(Debug, Default)]
pub struct TempoMarkerRaw {
    pub timepos: f64,
    pub measurepos: i32,
    pub beatpos: f64,
    pub bpm: f64,
    pub timesig_num: i32,
    pub timesig_denom: i32,
    pub lineartempo: bool,
}

/// Read a tempo/time-signature marker by index.
///
/// Returns `None` if the index is out of range.
pub fn get_tempo_marker(
    low: &ReaperLow,
    project: ProjectContext,
    index: i32,
) -> Option<TempoMarkerRaw> {
    let mut m = TempoMarkerRaw {
        bpm: 120.0,
        ..Default::default()
    };
    let exists = unsafe {
        low.GetTempoTimeSigMarker(
            project.to_raw(),
            index,
            &mut m.timepos,
            &mut m.measurepos,
            &mut m.beatpos,
            &mut m.bpm,
            &mut m.timesig_num,
            &mut m.timesig_denom,
            &mut m.lineartempo,
        )
    };
    exists.then_some(m)
}

/// Set (or add) a tempo/time-signature marker.
///
/// Pass `index = -1` to add a new marker.
#[allow(clippy::too_many_arguments)]
pub fn set_tempo_marker(
    low: &ReaperLow,
    project: ProjectContext,
    index: i32,
    timepos: f64,
    measurepos: i32,
    beatpos: f64,
    bpm: f64,
    timesig_num: i32,
    timesig_denom: i32,
    lineartempo: bool,
) -> bool {
    unsafe {
        low.SetTempoTimeSigMarker(
            project.to_raw(),
            index,
            timepos,
            measurepos,
            beatpos,
            bpm,
            timesig_num,
            timesig_denom,
            lineartempo,
        )
    }
}

/// Delete a tempo/time-signature marker.
pub fn delete_tempo_marker(low: &ReaperLow, project: ProjectContext, index: i32) {
    unsafe {
        low.DeleteTempoTimeSigMarker(project.to_raw(), index);
    }
}
